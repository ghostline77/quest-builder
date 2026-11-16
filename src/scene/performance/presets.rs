use super::*;

pub(super) enum Beat {
    Body {
        role: &'static str,
        start: u32,
        duration: u32,
        clip: &'static str,
        motion: bool,
    },
    Facial {
        start: u32,
        duration: u32,
        emotion: &'static str,
        expression: Option<&'static str>,
        scale: f64,
        clip_end: f64,
    },
    Look {
        actor: &'static str,
        target: &'static str,
        start: u32,
        chest: Option<f64>,
    },
    Correction {
        start: u32,
        duration: u32,
        bones: &'static [Bone],
    },
    Signal {
        port: &'static str,
        start: u32,
    },
}
pub(super) struct Bone {
    pub name: &'static str,
    pub rotation: [f64; 4],
    pub reference: [f64; 4],
}
pub(super) struct Binding {
    pub role: &'static str,
    pub rig: &'static str,
    pub set: &'static str,
    pub priority: u8,
}
impl Binding {
    pub fn path(&self) -> String {
        format!(
            "base\\animations\\npc\\generic_characters\\{}\\interactive_scene\\{}.anims",
            self.rig, self.set
        )
    }
}
pub(super) struct Recipe {
    pub duration: u32,
    pub roles: Vec<(&'static str, bool)>,
    pub ports: Vec<&'static str>,
    pub beats: Vec<Beat>,
    pub bindings: Vec<Binding>,
}
fn body(start: u32, duration: u32, clip: &'static str) -> Beat {
    Beat::Body {
        role: "speaker",
        start,
        duration,
        clip,
        motion: false,
    }
}
fn facial(
    start: u32,
    duration: u32,
    emotion: &'static str,
    expression: Option<&'static str>,
    scale: f64,
    clip_end: f64,
) -> Beat {
    Beat::Facial {
        start,
        duration,
        emotion,
        expression,
        scale,
        clip_end,
    }
}
fn look(start: u32, chest: f64) -> Beat {
    Beat::Look {
        actor: "speaker",
        target: "listener",
        start,
        chest: Some(chest),
    }
}
const FIRST_CORRECTION: &[Bone] = &[
    Bone {
        name: "RightArm",
        rotation: [-0.127432004, -0.214588001, 0.0282509997, 0.967944026],
        reference: [0.113753192, 0.609359324, -0.348788649, 0.703261018],
    },
    Bone {
        name: "LeftArm",
        rotation: [-0.0075960001, -0.0868239999, -0.0868239999, 0.992403984],
        reference: [0.189515561, 0.492123157, -0.561044931, 0.638447642],
    },
    Bone {
        name: "Spine1",
        rotation: [0., 0.130526006, 0., 0.991445005],
        reference: [0.0172426347, -0.0273420904, -0.0257451795, 0.999390125],
    },
];
const SECOND_CORRECTION: &[Bone] = &[
    Bone {
        name: "LeftArm",
        rotation: [-0.011376, -0.130030006, -0.0864100009, 0.987671971],
        reference: [0.0764463767, 0.352450222, -0.555420101, 0.749601245],
    },
    Bone {
        name: "RightArm",
        rotation: [-0.0868239999, -0.0868239999, 0.0075960001, 0.992403984],
        reference: [0.0633349493, 0.397948831, -0.496301055, 0.768925786],
    },
];

pub(super) fn recipe(name: &str) -> Result<Recipe> {
    let mut recipe = Recipe {
        duration: 0,
        roles: vec![("speaker", false), ("listener", true)],
        ports: Vec::new(),
        beats: Vec::new(),
        bindings: vec![Binding {
            role: "speaker",
            rig: "male_big",
            set: "generic_male_big__sit_chair__2h_on_lap__01",
            priority: 200,
        }],
    };
    match name {
        "seated_sad_reply" => {
            recipe.duration = 18039;
            recipe.beats = vec![
                body(444, 3600, "sit_chair__2h_on_lap__01__what__sad__01"),
                look(795, 2.),
                facial(2862, 2610, "anger", None, 0.461130768, 0.),
                facial(
                    4840,
                    3843,
                    "disinterested",
                    Some("uncertain"),
                    0.678975344,
                    0.,
                ),
                body(7388, 9367, "sit_chair__2h_on_lap__01__no__sad__02"),
                facial(7717, 2777, "sadness", None, 0.640571594, 1.32480884),
                facial(13163, 4210, "sadness", Some("full"), 0.743816257, 0.),
            ];
        }
        "seated_explanation" => {
            recipe.duration = 29031;
            recipe.beats = vec![
                look(0, 2.),
                body(5149, 5000, "sit_chair__2h_on_lap__01__talk__neutral__03"),
                facial(17322, 3240, "neutral", None, 1., 0.),
                body(
                    22675,
                    4567,
                    "sit_chair__2h_on_lap__01__this_or_that__neutral__01",
                ),
            ];
        }
        "arrive_and_sit" => {
            recipe.duration = 4300;
            recipe.bindings[0].set = "generic_big_male_transitions";
            recipe.ports = vec!["settled"];
            recipe.beats = vec![
                Beat::Body {
                    role: "speaker",
                    start: 0,
                    duration: 4300,
                    clip: "walk_0__to__sit_chair__2h_on_lap__01__turn180l__01",
                    motion: true,
                },
                facial(233, 3677, "interested", Some("suspicious"), 0.649646699, 0.),
                Beat::Correction {
                    start: 882,
                    duration: 367,
                    bones: FIRST_CORRECTION,
                },
                look(1700, 2.),
                Beat::Correction {
                    start: 2087,
                    duration: 316,
                    bones: SECOND_CORRECTION,
                },
                Beat::Correction {
                    start: 2748,
                    duration: 367,
                    bones: &[],
                },
                Beat::Signal {
                    port: "settled",
                    start: 4299,
                },
            ];
        }
        "stand_from_chair" => {
            recipe.duration = 5067;
            recipe.bindings[0].set = "generic_big_male_transitions";
            recipe.ports = vec!["idle_only", "standing"];
            recipe.beats = vec![
                Beat::Body {
                    role: "speaker",
                    start: 0,
                    duration: 5067,
                    clip: "sit_chair__2h_on_lap__01__to__stand__2h_on_sides__01__turn0__01",
                    motion: true,
                },
                Beat::Signal {
                    port: "idle_only",
                    start: 0,
                },
                facial(
                    824,
                    3860,
                    "disinterested",
                    Some("uncertain"),
                    0.681978822,
                    0.,
                ),
                look(1102, 0.799999952),
                Beat::Signal {
                    port: "standing",
                    start: 5066,
                },
            ];
        }
        "group_side_exchange" => {
            recipe.duration = 9818;
            recipe.roles = vec![("smoker", false), ("table_listener", false)];
            recipe.bindings = vec![
                Binding {
                    role: "smoker",
                    rig: "male_big",
                    set: "generic_male_big__stand_wall_lean180__rh_cigarette__01",
                    priority: 128,
                },
                Binding {
                    role: "table_listener",
                    rig: "male_average",
                    set: "generic_male_average__stand_table_lean0__2h_on_table__01",
                    priority: 200,
                },
            ];
            recipe.beats = vec![
                Beat::Body {
                    role: "smoker",
                    start: 0,
                    duration: 1967,
                    clip: "stand_wall_lean180__rh_cigarette__01__shrug__01",
                    motion: false,
                },
                Beat::Look {
                    actor: "table_listener",
                    target: "smoker",
                    start: 806,
                    chest: None,
                },
                Beat::Body {
                    role: "table_listener",
                    start: 2353,
                    duration: 3767,
                    clip: "stand_table_lean0__2h_on_table__01__describe_left__01",
                    motion: false,
                },
                Beat::Body {
                    role: "smoker",
                    start: 5443,
                    duration: 4233,
                    clip: "stand_wall_lean180__rh_cigarette__01__talk__neutral__03",
                    motion: false,
                },
            ];
        }
        _ => anyhow::bail!("unknown performance preset {name}"),
    }
    Ok(recipe)
}
