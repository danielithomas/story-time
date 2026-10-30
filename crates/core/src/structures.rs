//! Story structures used to lay out a project's flow note.
//!
//! Each beat carries a position as a percentage of the story. With a word target, the flow note
//! shows roughly where in the word count each beat lands.

use crate::text::thousands;

pub struct Structure {
    pub key: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub parts: &'static [Part],
}

pub struct Part {
    pub name: &'static str,
    /// Start and end, as percentages of the story.
    pub span: Option<(u8, u8)>,
    pub prompt: &'static str,
    pub beats: &'static [Beat],
}

pub struct Beat {
    pub name: &'static str,
    pub span: (u8, u8),
    pub prompt: &'static str,
}

const fn beat(name: &'static str, from: u8, to: u8, prompt: &'static str) -> Beat {
    Beat {
        name,
        span: (from, to),
        prompt,
    }
}

pub const THREE_ACT: Structure = Structure {
    key: "three-act",
    name: "Three-act structure",
    summary: "Setup, confrontation, resolution, with turning points at 25%, 50% and 75%",
    parts: &[
        Part {
            name: "Act One: Setup",
            span: Some((0, 25)),
            prompt: "Who the story is about, the world they live in, and what is missing.",
            beats: &[
                beat(
                    "Hook",
                    0,
                    1,
                    "Open with something that makes the reader lean in: a question, a voice, a situation.",
                ),
                beat(
                    "Inciting incident",
                    10,
                    10,
                    "The event that disrupts the ordinary world and sets the story moving.",
                ),
                beat(
                    "Plot point one",
                    25,
                    25,
                    "The protagonist commits. A door closes behind them and the story proper begins.",
                ),
            ],
        },
        Part {
            name: "Act Two: Confrontation",
            span: Some((25, 75)),
            prompt: "The protagonist pursues the goal; opposition grows.",
            beats: &[
                beat(
                    "Rising action",
                    25,
                    50,
                    "Attempts, setbacks and complications. The stakes climb.",
                ),
                beat(
                    "Midpoint",
                    50,
                    50,
                    "A reversal or revelation changes the protagonist's understanding: they stop reacting and start acting.",
                ),
                beat(
                    "Tightening",
                    50,
                    75,
                    "Opposition closes in, costs mount, plans fail.",
                ),
                beat(
                    "Plot point two",
                    75,
                    75,
                    "The lowest point, or a discovery that makes the final confrontation inevitable.",
                ),
            ],
        },
        Part {
            name: "Act Three: Resolution",
            span: Some((75, 100)),
            prompt: "Everything converges.",
            beats: &[
                beat(
                    "Climax",
                    90,
                    90,
                    "The decisive confrontation. The protagonist acts on everything they have learned.",
                ),
                beat("Resolution", 98, 100, "The new normal. Show what changed."),
            ],
        },
    ],
};

pub const SAVE_THE_CAT: Structure = Structure {
    key: "save-the-cat",
    name: "Save the Cat (15 beats)",
    summary: "Blake Snyder's 15-beat sheet, as adapted for novels by Jessica Brody",
    parts: &[
        Part {
            name: "Act One: Thesis",
            span: Some((0, 20)),
            prompt: "The world as it is, and the hero who needs to change.",
            beats: &[
                beat(
                    "Opening image",
                    0,
                    1,
                    "A snapshot of the hero and their world before the story changes them.",
                ),
                beat(
                    "Theme stated",
                    5,
                    5,
                    "Someone hints at the lesson the hero must learn, usually without the hero noticing.",
                ),
                beat(
                    "Setup",
                    1,
                    10,
                    "The hero's ordinary life, flaws, and the things that need fixing.",
                ),
                beat(
                    "Catalyst",
                    10,
                    10,
                    "The life-changing event that knocks the hero out of the status quo.",
                ),
                beat(
                    "Debate",
                    10,
                    20,
                    "The hero hesitates. Should they go? What will it cost?",
                ),
            ],
        },
        Part {
            name: "Act Two: Antithesis",
            span: Some((20, 80)),
            prompt: "An upside-down version of the hero's world.",
            beats: &[
                beat(
                    "Break into two",
                    20,
                    20,
                    "The hero chooses to act and enters the new world.",
                ),
                beat(
                    "B story",
                    22,
                    22,
                    "A new character or relationship that carries the theme.",
                ),
                beat(
                    "Fun and games",
                    20,
                    50,
                    "The promise of the premise: what the reader came for.",
                ),
                beat(
                    "Midpoint",
                    50,
                    50,
                    "A false victory or false defeat. The stakes rise.",
                ),
                beat(
                    "Bad guys close in",
                    50,
                    75,
                    "External pressure and internal flaws tighten the screws.",
                ),
                beat(
                    "All is lost",
                    75,
                    75,
                    "The lowest point, often with a 'whiff of death'.",
                ),
                beat(
                    "Dark night of the soul",
                    75,
                    80,
                    "The hero wallows, then glimpses the truth they have avoided.",
                ),
            ],
        },
        Part {
            name: "Act Three: Synthesis",
            span: Some((80, 100)),
            prompt: "The hero combines what they were with what they have learned.",
            beats: &[
                beat(
                    "Break into three",
                    80,
                    80,
                    "The hero sees what must be done; theme and plot converge.",
                ),
                beat(
                    "Finale",
                    80,
                    99,
                    "The hero carries out the new plan and proves they have changed.",
                ),
                beat(
                    "Final image",
                    99,
                    100,
                    "A mirror of the opening image that shows the transformation.",
                ),
            ],
        },
    ],
};

pub const HEROS_JOURNEY: Structure = Structure {
    key: "heros-journey",
    name: "The Hero's Journey (12 stages)",
    summary: "Christopher Vogler's twelve stages, after Joseph Campbell",
    parts: &[
        Part {
            name: "Departure",
            span: Some((0, 25)),
            prompt: "The hero leaves the known world.",
            beats: &[
                beat(
                    "The ordinary world",
                    0,
                    10,
                    "The hero's everyday life, and the lack or wound they carry.",
                ),
                beat(
                    "The call to adventure",
                    10,
                    10,
                    "A problem, challenge or invitation upsets the ordinary.",
                ),
                beat(
                    "Refusal of the call",
                    12,
                    12,
                    "Fear or obligation makes the hero hesitate.",
                ),
                beat(
                    "Meeting the mentor",
                    17,
                    17,
                    "Someone or something offers advice, training or a gift.",
                ),
                beat(
                    "Crossing the threshold",
                    25,
                    25,
                    "The hero commits and enters the special world.",
                ),
            ],
        },
        Part {
            name: "Initiation",
            span: Some((25, 75)),
            prompt: "The hero is tested and transformed.",
            beats: &[
                beat(
                    "Tests, allies and enemies",
                    25,
                    40,
                    "The hero learns the rules of the new world; friends and foes emerge.",
                ),
                beat(
                    "Approach to the inmost cave",
                    40,
                    50,
                    "Preparations for the central ordeal as danger draws near.",
                ),
                beat(
                    "The ordeal",
                    50,
                    50,
                    "A crisis where the hero faces their greatest fear: a death and rebirth.",
                ),
                beat(
                    "The reward",
                    55,
                    65,
                    "Having survived, the hero seizes the treasure, knowledge or reconciliation.",
                ),
            ],
        },
        Part {
            name: "Return",
            span: Some((75, 100)),
            prompt: "The hero brings what they have won back to the world.",
            beats: &[
                beat(
                    "The road back",
                    75,
                    75,
                    "The hero recommits to finishing the journey; consequences pursue them.",
                ),
                beat(
                    "Resurrection",
                    90,
                    90,
                    "A final, highest-stakes test where the hero must use everything they have learned.",
                ),
                beat(
                    "Return with the elixir",
                    98,
                    100,
                    "The hero returns transformed, bringing something that benefits their world.",
                ),
            ],
        },
    ],
};

pub const STORY_CIRCLE: Structure = Structure {
    key: "story-circle",
    name: "Story Circle (8 steps)",
    summary: "Dan Harmon's simplification of the hero's journey: order, chaos, order",
    parts: &[
        Part {
            name: "Order",
            span: Some((0, 25)),
            prompt: "The familiar world.",
            beats: &[
                beat("You", 0, 0, "A character in a zone of comfort."),
                beat("Need", 12, 12, "They want something."),
            ],
        },
        Part {
            name: "Chaos",
            span: Some((25, 75)),
            prompt: "The unfamiliar world.",
            beats: &[
                beat("Go", 25, 25, "They enter an unfamiliar situation."),
                beat("Search", 25, 50, "They adapt to it."),
                beat("Find", 50, 50, "They get what they wanted."),
                beat("Take", 62, 75, "They pay a heavy price for it."),
            ],
        },
        Part {
            name: "Order restored",
            span: Some((75, 100)),
            prompt: "The familiar world, seen with new eyes.",
            beats: &[
                beat("Return", 75, 75, "They return to their familiar situation."),
                beat("Change", 88, 100, "Having changed."),
            ],
        },
    ],
};

pub const KISHOTENKETSU: Structure = Structure {
    key: "kishotenketsu",
    name: "Kishōtenketsu (4 parts)",
    summary: "East Asian four-part structure built on a twist rather than on conflict",
    parts: &[
        Part {
            name: "Ki: introduction",
            span: Some((0, 25)),
            prompt: "Characters, setting and situation.",
            beats: &[],
        },
        Part {
            name: "Shō: development",
            span: Some((25, 50)),
            prompt: "Deepen what was introduced, without a major turn yet.",
            beats: &[],
        },
        Part {
            name: "Ten: twist",
            span: Some((50, 75)),
            prompt: "An unexpected turn or new element that recasts everything before it.",
            beats: &[],
        },
        Part {
            name: "Ketsu: reconciliation",
            span: Some((75, 100)),
            prompt: "Bring the elements together so the reader sees the whole anew.",
            beats: &[],
        },
    ],
};

pub const FREEFORM: Structure = Structure {
    key: "freeform",
    name: "Freeform",
    summary: "No structure: an empty timeline",
    parts: &[Part {
        name: "Timeline",
        span: None,
        prompt: "Chapters and scenes in story order.",
        beats: &[],
    }],
};

pub const STRUCTURES: [&Structure; 6] = [
    &THREE_ACT,
    &SAVE_THE_CAT,
    &HEROS_JOURNEY,
    &STORY_CIRCLE,
    &KISHOTENKETSU,
    &FREEFORM,
];

pub fn find(key: &str) -> Option<&'static Structure> {
    STRUCTURES
        .into_iter()
        .find(|structure| structure.key.eq_ignore_ascii_case(key))
}

/// The structure as Markdown headings, with each beat's position and prompt.
pub fn render(structure: &Structure, target: Option<u64>) -> String {
    let mut out = String::new();
    for part in structure.parts {
        out.push_str(&format!("## {}\n\n", part.name));
        if let Some(span) = part.span {
            out.push_str(&format!("*{}*\n\n", position(span, target)));
        }
        out.push_str(&format!("%% {} %%\n\n", part.prompt));
        for beat in part.beats {
            out.push_str(&format!(
                "### {}\n\n*{}*\n\n%% {} %%\n\n",
                beat.name,
                position(beat.span, target),
                beat.prompt
            ));
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

fn position((from, to): (u8, u8), target: Option<u64>) -> String {
    let percent = if from == to {
        format!("{from}%")
    } else {
        format!("{from}–{to}%")
    };
    let Some(target) = target.filter(|target| *target > 0) else {
        return percent;
    };
    let at = |p: u8| {
        let words = target * u64::from(p) / 100;
        let rounded = if target >= 10_000 {
            (words + 50) / 100 * 100
        } else {
            (words + 5) / 10 * 10
        };
        thousands(rounded as i64)
    };
    if from == to {
        format!("{percent} · around {} words", at(from))
    } else {
        format!("{percent} · words {}–{}", at(from), at(to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_findable() {
        for structure in STRUCTURES {
            assert_eq!(find(structure.key).unwrap().name, structure.name);
        }
        assert!(find("Save-The-Cat").is_some());
        assert!(find("nonsense").is_none());
    }

    #[test]
    fn renders_word_positions() {
        let text = render(&SAVE_THE_CAT, Some(90_000));
        assert!(
            text.contains(
                "### Catalyst\n\n*10% · around 9,000 words*\n\n%% The life-changing event"
            )
        );
        assert!(text.contains("## Act Two: Antithesis\n\n*20–80% · words 18,000–72,000*"));
        assert!(text.ends_with("%%\n"));
        let short = render(&THREE_ACT, Some(5_000));
        assert!(short.contains("*10% · around 500 words*"));
        let untargeted = render(&THREE_ACT, None);
        assert!(untargeted.contains("### Midpoint\n\n*50%*\n"));
    }

    #[test]
    fn freeform_has_no_positions() {
        assert_eq!(
            render(&FREEFORM, Some(90_000)),
            "## Timeline\n\n%% Chapters and scenes in story order. %%\n"
        );
    }
}
