use super::{
    Handles,
    geometry::{self, Position},
    nodes::{self, list},
};
use crate::nodes as n;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

/// A Bezier segment stays inside its control hull. Include handles, rotation,
/// scale and road width so local streaming bounds cannot cut off the route.
pub(super) fn bounds(
    node: &Value,
    spec: &Value,
    anchor: super::geometry::Anchor,
) -> Result<Vec<Position>> {
    let data = &node["Data"];
    let Some(points) = data
        .pointer("/splineData/Data/points")
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };
    let empty = json!({});
    let overrides = spec.get("node_data").unwrap_or(&empty);
    let q = overrides
        .get("orientation")
        .cloned()
        .unwrap_or_else(|| geometry::quaternion(anchor.yaw));
    let [i, j, k, r] = ["i", "j", "k", "r"]
        .map(|key| geometry::number(&q[key]))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| anyhow::anyhow!("quaternion components"))?;
    let norm = i * i + j * j + k * k + r * r;
    ensure!(norm.is_finite() && norm > 0., "invalid spline orientation");
    let sx = geometry::scalar(overrides, "scale_x", 1.)?;
    let sy = geometry::scalar(overrides, "scale_y", 1.)?;
    let sz = geometry::scalar(overrides, "scale_z", 1.)?;
    let read = |value: &Value| -> Result<Position> {
        Ok(Position {
            x: geometry::number(&value["X"])?,
            y: geometry::number(&value["Y"])?,
            z: geometry::number(&value["Z"])?,
        })
    };
    let transform = |p: Position| {
        let (x, y, z) = (p.x * sx, p.y * sy, p.z * sz);
        Position {
            x: anchor.position.x
                + ((r * r + i * i - j * j - k * k) * x
                    + 2. * (i * j - r * k) * y
                    + 2. * (i * k + r * j) * z)
                    / norm,
            y: anchor.position.y
                + (2. * (i * j + r * k) * x
                    + (r * r - i * i + j * j - k * k) * y
                    + 2. * (j * k - r * i) * z)
                    / norm,
            z: anchor.position.z
                + (2. * (i * k - r * j) * x
                    + 2. * (j * k + r * i) * y
                    + (r * r - i * i - j * j + k * k) * z)
                    / norm,
        }
    };
    let mut width = data
        .get("offsetDefault")
        .and_then(Value::as_f64)
        .unwrap_or(0.);
    if let Some(sections) = data.get("offsets").and_then(Value::as_array) {
        for section in sections {
            for side in ["left", "right"] {
                width = width.max(geometry::number(&section[side])?.abs());
            }
        }
    }
    width *= sx.abs().max(sy.abs()).max(sz.abs());
    let mut result = Vec::new();
    for point in points {
        let p = read(&point["position"])?;
        let mut hull = vec![p];
        for tangent in point["tangents"]["Elements"]
            .as_array()
            .context("spline tangent hull")?
        {
            let t = read(tangent)?;
            hull.push(p.offset(t.x, t.y, t.z));
        }
        for p in hull {
            let p = transform(p);
            result.push(p.offset(-width, -width, -width));
            result.push(p.offset(width, width, width));
        }
    }
    Ok(result)
}

fn sections(spec: &Value, widths: bool) -> Result<Vec<Value>> {
    let key = if widths {
        "width_sections"
    } else {
        "speed_sections"
    };
    let mut previous = -1.;
    let mut result = Vec::new();
    for section in list(spec, key)? {
        let start = geometry::scalar(section, if widths { "from" } else { "start" }, 0.)?;
        let end = geometry::scalar(section, if widths { "to" } else { "end" }, start)?;
        ensure!(
            start >= 0. && end >= start && (!widths || end > start) && start >= previous,
            "spline tuning sections must be ordered and non-overlapping"
        );
        previous = end;
        let value = if widths {
            let left = geometry::number(&section["left"])?;
            let right = geometry::number(&section["right"])?;
            ensure!(
                left >= 0. && right >= 0.,
                "spline widths must be nonnegative"
            );
            json!({"$type":"worldRaceSplineNodeOffset","from":start,"to":end,"left":left,"right":right})
        } else {
            let speed = geometry::number(&section["speed_mps"])?;
            ensure!(speed >= 0., "spline speed must be nonnegative");
            json!({"$type":"worldSpeedSplineNodeSpeedChangeSection","start":start,"end":end,"targetSpeed_M_P_S":speed})
        };
        result.push(value);
    }
    Ok(result)
}
pub(super) fn build(spec: &Value, reference: &str, h: &mut Handles, race: bool) -> Result<Value> {
    let raw = list(spec, "points")?;
    ensure!(raw.len() >= 2, "spline requires at least two points");
    let points = raw
        .iter()
        .map(|v| Position::read(v.get("position").unwrap_or(v)))
        .collect::<Result<Vec<_>>>()?;
    let looped = nodes::flag(spec, "looped", false)?;
    let mut emitted = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let previous = if index > 0 {
            points.get(index - 1)
        } else if looped {
            points.last()
        } else {
            None
        };
        let next = if index + 1 < points.len() {
            points.get(index + 1)
        } else if looped {
            points.first()
        } else {
            None
        };
        let zero = Position {
            x: 0.,
            y: 0.,
            z: 0.,
        };
        let mut incoming = previous.map_or(zero, |v| Position {
            x: (v.x - point.x) / 3.,
            y: (v.y - point.y) / 3.,
            z: (v.z - point.z) / 3.,
        });
        let mut outgoing = next.map_or(zero, |v| Position {
            x: (v.x - point.x) / 3.,
            y: (v.y - point.y) / 3.,
            z: (v.z - point.z) / 3.,
        });
        if let Some(tangents) = raw[index].get("tangents") {
            let tangents = tangents.as_array().context("spline tangents")?;
            ensure!(
                tangents.len() == 2,
                "each spline point needs two tangent vectors"
            );
            // Native SplinePoint stores incoming then outgoing handles.
            // Explicit handles already use native order and must stay untouched.
            incoming = Position::read(&tangents[0])?;
            outgoing = Position::read(&tangents[1])?;
        }
        emitted.push(json!({"$type":"SplinePoint","automaticTangents":raw[index].get("automatic_tangents").cloned().unwrap_or(json!(0)),"continuousTangents":1,"id":0,"position":point.vector(),"rotation":geometry::quaternion(0.),"tangents":{"Elements":[incoming.vector(),outgoing.vector()]}}));
    }
    let patrol = list(spec, "patrol_spots")?;
    ensure!(
        !race || patrol.is_empty(),
        "race spline cannot contain patrol activity spots"
    );
    let mut data = nodes::common(
        spec,
        reference,
        if patrol.is_empty() {
            "worldSplineNode"
        } else {
            "worldPatrolSplineNode"
        },
    );
    data["destSnapedNode"] = nodes::zero_ref();
    data["destSnapedSocketName"] = n::cname("None");
    data["entrySnapedNode"] = nodes::zero_ref();
    data["entrySnapedSocketName"] = n::cname("None");
    data["splineData"]=h.wrap(json!({"$type":"Spline","hasDirection":u8::from(nodes::flag(spec,"has_direction",true)?),"looped":u8::from(looped),"points":emitted,"reversed":u8::from(nodes::flag(spec,"reversed",false)?)}));
    if !patrol.is_empty() {
        let mut definitions = Vec::new();
        for spot in patrol {
            definitions.push(h.wrap(json!({"$type":"worldPatrolSplinePointDefinition","node":n::node_ref(spot.as_str().context("patrol workspot reference")?),"pointType":"Workspot","target":n::entity(None,&[])})));
        }
        data["patrolPointDefs"] = json!(definitions);
        data["patrolPoints"] = json!([]);
        data["spots"] = json!([]);
    }
    if spec.get("speed_sections").is_some() && patrol.is_empty() {
        data["$type"] = json!("worldSpeedSplineNode");
        data["speedChangeSections"] = json!(sections(spec, false)?);
    }
    if race {
        let width = geometry::scalar(spec, "width_default", 5.)?;
        ensure!(width >= 0., "race width must be nonnegative");
        data.as_object_mut().context("race spline")?.extend(json!({"$type":"worldRaceSplineNode","speedChangeSections":sections(spec,false)?,"orientationChangeSections":[],"roadAdjustmentFactorChangeSections":[],"deprecatedSpeedRestrictions":[],"deprecatedDefaultSpeed":0.,"deprecatedDefaultAdjustTime":0.,"useDeprecated":0,"ignoreTerrain":0,"offsets":sections(spec,true)?,"offsetDefault":width}).as_object().context("race fields")?.clone());
    }
    Ok(h.wrap(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_handles_follow_the_route_without_backtracking() {
        let mut handles = Handles::default();
        let node = build(
            &json!({"points":[[0.,0.,0.],[9.,0.,0.],[18.,0.,0.]]}),
            "#route",
            &mut handles,
            true,
        )
        .unwrap();
        let points = node
            .pointer("/Data/splineData/Data/points")
            .unwrap()
            .as_array()
            .unwrap();
        // Native segment evaluation uses the start's handle 1 and end's handle 0.
        let first = points[0]["tangents"]["Elements"][1]["X"].as_f64().unwrap();
        let second = 9. + points[1]["tangents"]["Elements"][0]["X"].as_f64().unwrap();
        assert_eq!(first, 3.);
        assert_eq!(second, 6.);
        for i in 0..=20 {
            let t = i as f64 / 20.;
            let position =
                3. * (1. - t).powi(2) * t * first + 3. * (1. - t) * t * t * second + t.powi(3) * 9.;
            assert!((position - 9. * t).abs() < 1e-10);
        }
    }

    #[test]
    fn explicit_native_handles_keep_their_authored_order() {
        let mut handles = Handles::default();
        let node=build(&json!({"points":[{"position":[0.,0.,0.],"tangents":[[2.,1.,0.],[-4.,-1.,0.]]},[9.,0.,0.]]}),"#route",&mut handles,false).unwrap();
        let tangents = &node["Data"]["splineData"]["Data"]["points"][0]["tangents"]["Elements"];
        assert_eq!(tangents[0]["X"], 2.);
        assert_eq!(tangents[1]["X"], -4.);
    }
}
