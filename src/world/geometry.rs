use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{collections::BTreeMap, f64::consts::TAU};

#[derive(Clone, Copy, Debug)]
pub(super) struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Position {
    pub fn read(v: &Value) -> Result<Self> {
        let p = if let Some(v) = v.as_array() {
            ensure!(v.len() == 3, "position needs exactly three coordinates");
            Self {
                x: number(&v[0])?,
                y: number(&v[1])?,
                z: number(&v[2])?,
            }
        } else {
            Self {
                x: number(&v["x"])?,
                y: number(&v["y"])?,
                z: number(&v["z"])?,
            }
        };
        Ok(p)
    }
    pub fn vector(self) -> Value {
        json!({"$type":"Vector3","X":self.x,"Y":self.y,"Z":self.z})
    }
    pub fn vector4(self, w: f64) -> Value {
        json!({"$type":"Vector4","X":self.x,"Y":self.y,"Z":self.z,"W":w})
    }
    pub fn offset(self, x: f64, y: f64, z: f64) -> Self {
        Self {
            x: self.x + x,
            y: self.y + y,
            z: self.z + z,
        }
    }
}
pub(super) fn number(v: &Value) -> Result<f64> {
    let n = v.as_f64().context("finite numeric coordinate")?;
    ensure!(n.is_finite(), "coordinate must be finite");
    Ok(n)
}
pub(super) fn scalar(v: &Value, key: &str, default: f64) -> Result<f64> {
    v.get(key).map_or(Ok(default), number)
}
#[derive(Clone, Copy)]
pub(super) struct Anchor {
    pub position: Position,
    pub yaw: f64,
    pub index: Option<usize>,
}

/// Native NodeRef FNV identity, excluding alias markers.
pub fn hash(value: &str) -> u64 {
    if value.is_empty() {
        return 0;
    }
    let units = value.encode_utf16().collect::<Vec<_>>();
    let mut index = 0;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    while index < units.len() {
        if units[index] == u16::from(b'#') {
            index += 1;
            if units.get(index) == Some(&u16::from(b';')) {
                while index < units.len() && units[index] != u16::from(b'/') {
                    index += 1;
                }
            }
        }
        if index >= units.len() {
            break;
        }
        hash ^= u64::from(units[index]);
        hash = hash.wrapping_mul(0x100_0000_01b3);
        index += 1;
    }
    hash
}
pub(super) fn full_ref(root: &str, value: &str) -> String {
    if value.starts_with("$/") {
        value.into()
    } else {
        format!(
            "{}/{}{}",
            root.trim_end_matches('/'),
            if value.starts_with('#') { "" } else { "#" },
            value
        )
    }
}
pub(super) fn yaw(
    value: Option<&Value>,
    anchors: &BTreeMap<String, Anchor>,
    default: f64,
) -> Result<f64> {
    match value {
        None | Some(Value::Null) => Ok(default),
        Some(Value::String(s)) if matches!(s.as_str(), "origin" | "anchor") => Ok(default),
        Some(Value::String(s)) => Ok(lookup(anchors, s)?.yaw),
        Some(v) => number(v),
    }
}
pub(super) fn lookup<'a>(anchors: &'a BTreeMap<String, Anchor>, key: &str) -> Result<&'a Anchor> {
    anchors
        .get(key)
        .or_else(|| anchors.get(key.trim_start_matches('#')))
        .or_else(|| anchors.get(&format!("#{key}")))
        .with_context(|| format!("unknown world anchor {key}"))
}
pub(super) fn position(
    value: Option<&Value>,
    anchors: &BTreeMap<String, Anchor>,
) -> Result<Anchor> {
    let origin = *lookup(anchors, "origin")?;
    let Some(v) = value.filter(|v| !v.is_null()) else {
        return Ok(origin);
    };
    if v.is_array() || ["x", "y", "z"].iter().all(|k| v.get(*k).is_some()) {
        return Ok(Anchor {
            position: Position::read(v)?,
            yaw: origin.yaw,
            index: None,
        });
    }
    ensure!(v.is_object(), "relative position must be an object");
    let anchor = *lookup(
        anchors,
        v.get("from").and_then(Value::as_str).unwrap_or("origin"),
    )?;
    let yaw = yaw(v.get("yaw"), anchors, anchor.yaw)?;
    let mut forward = scalar(v, "forward", 0.)?;
    let mut right = scalar(v, "right", 0.)?;
    let up = scalar(v, "up", scalar(v, "z_offset", 0.)?)?;
    if let Some(distance) = v.get("distance") {
        let distance = number(distance)?;
        let bearing = scalar(v, "bearing", 0.)?.to_radians();
        forward += bearing.cos() * distance;
        right += bearing.sin() * distance;
    }
    let a = yaw.to_radians();
    Ok(Anchor {
        position: anchor.position.offset(
            a.cos() * forward - a.sin() * right,
            a.sin() * forward + a.cos() * right,
            up,
        ),
        yaw,
        index: None,
    })
}
pub(super) fn quaternion(yaw: f64) -> Value {
    let half = yaw.to_radians() / 2.;
    json!({"$type":"Quaternion","i":0,"j":0,"k":half.sin(),"r":half.cos()})
}
fn outline_points(spec: &Value) -> Result<(Vec<Position>, f64)> {
    let height = scalar(spec, "height", 2.)?;
    ensure!(height > 0., "outline height must be positive");
    let points = if let Some(points) = spec.get("points") {
        points
            .as_array()
            .context("outline points")?
            .iter()
            .map(Position::read)
            .collect::<Result<Vec<_>>>()?
    } else {
        match spec
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("rectangle")
            .to_ascii_lowercase()
            .as_str()
        {
            "rectangle" | "box" | "square" => {
                let width = scalar(spec, "width", scalar(spec, "size", 2.)?)?;
                let depth = scalar(
                    spec,
                    "depth",
                    scalar(spec, "length", scalar(spec, "size", width)?)?,
                )?;
                ensure!(
                    width > 0. && depth > 0.,
                    "outline dimensions must be positive"
                );
                vec![
                    Position {
                        x: -width / 2.,
                        y: -depth / 2.,
                        z: 0.,
                    },
                    Position {
                        x: width / 2.,
                        y: -depth / 2.,
                        z: 0.,
                    },
                    Position {
                        x: width / 2.,
                        y: depth / 2.,
                        z: 0.,
                    },
                    Position {
                        x: -width / 2.,
                        y: depth / 2.,
                        z: 0.,
                    },
                ]
            }
            "circle" | "disc" | "regular_polygon" => {
                let radius = scalar(spec, "radius", 1.)?;
                let count = spec
                    .get("points_count")
                    .or_else(|| spec.get("segments"))
                    .map_or(Ok(12), |v| v.as_u64().context("polygon segments"))?;
                ensure!(
                    (3..=65536).contains(&count) && radius > 0.,
                    "invalid polygon radius/segments"
                );
                (0..count)
                    .map(|i| {
                        let angle = TAU * (i as f64) / (count as f64);
                        Position {
                            x: angle.cos() * radius,
                            y: angle.sin() * radius,
                            z: 0.,
                        }
                    })
                    .collect()
            }
            kind => bail!("unsupported outline kind {kind}"),
        }
    };
    ensure!(points.len() >= 3, "outline needs at least three points");
    Ok((points, height))
}

/// RedPackage classes serialize the reflected points and height fields.
pub(super) fn package_outline(spec: &Value) -> Result<Value> {
    let (points, height) = outline_points(spec)?;
    Ok(
        json!({"$type":"AreaShapeOutline","height":height,"points":points.into_iter().map(Position::vector).collect::<Vec<_>>()}),
    )
}

pub(super) fn outline(spec: &Value) -> Result<Value> {
    let (points, height) = outline_points(spec)?;
    let mut bytes = u32::try_from(points.len())?.to_le_bytes().to_vec();
    for point in &points {
        for n in [point.x, point.y, point.z, 1.] {
            ensure!(
                n.abs() <= f64::from(f32::MAX),
                "outline coordinate exceeds float32"
            );
            bytes.extend_from_slice(&(n as f32).to_le_bytes());
        }
    }
    bytes.extend_from_slice(&(height as f32).to_le_bytes());
    Ok(json!({"$type":"AreaShapeOutline","buffer":STANDARD.encode(bytes)}))
}
