//! What this workload's numbers are allowed to be.

use zgui_bench::reference::verdict::{Allowed, Criterion};

use crate::gesture::Gesture;

/// The cost of a gesture over a hundred thousand rows, against its cost over twelve and a half.
///
/// The virtualisation claim, stated as a same-run ratio of two costs rather than as a slope,
/// because the slope's right answer is zero and a ratio against zero is not a number. Both costs
/// are medians of forty-eight passes over documents that realise the same rows in the same port; the
/// only thing that differs between them is how much data is behind the port.
///
/// A ceiling rather than a band: a ratio *under* one is the eightfold document coming out cheaper,
/// which is the measurement's own noise and not a regression anybody should be told about.
///
/// **Measured at 0.9970–1.0056 over three runs, on both gestures.** Eight times the data, the same
/// cost to the nanosecond of the spread. The ceiling is five per cent, which is well outside that
/// and an order of magnitude under what a list that had stopped virtualising would produce.
///
/// # What it can and cannot catch
///
/// It is a timing gate, so it has a noise floor and the floor is stated rather than implied. A
/// per-model-row cost planted in the virtualiser's own window computation moves the ratio to 1.028
/// when it is a bare loop over the rows and to **1.25 when each row costs a square root** — the
/// second fails, the first does not. So: a stage that became a function of the row count is caught
/// once it is worth about five per cent of a gesture, and a cheaper one hides under the spread.
///
/// The exact half of the same claim is not here and is not a criterion at all — it is the assertion
/// in `main.rs` that the four documents realised the *same number of rows*. That one has no noise
/// floor: a list that stopped virtualising fails it on the first size, whatever the clock says.
pub(crate) fn virtualisation(gesture: Gesture) -> Criterion {
    Criterion {
        name: match gesture {
            Gesture::Wheel => "LIST-virtualisation-wheel",
            Gesture::Touchpad => "LIST-virtualisation-touchpad",
        },
        subject: "one gesture over a hundred thousand rows",
        baseline: "the same gesture over twelve and a half thousand of them",
        allowed: Allowed::Under { most: 1.05 },
        advice: "Eight times the data now costs more to scroll. A virtualised list's cost is \
                 supposed to be a function of its port: look for something on the scroll path that \
                 is a function of the row count instead — a content height recomputed by walking \
                 rows, a scrollbar thumb measured from them, an observation delivered per row \
                 rather than per realised row.",
    }
}

/// The share of the surface one drawn frame of a gesture damages.
///
/// Dimensionless before anybody divides anything, so this is one of the two numbers here that is
/// the same on every machine. It is also the only evidence that separates a scroll which moved the
/// content from a scroll which redrew the page: on a machine fast enough the two take the same
/// time, and on every other machine they do not.
///
/// A list scrolled under its port damages the port, and the port is most of this window — so the
/// fraction is large and the interesting failure is not that it is large. **The wheel reads 0.9460
/// and the touchpad 0.4830**, identically on every run, because a damage fraction is arithmetic on
/// a frame rather than a sample of anything. The two gestures differ because they draw different
/// frames: a discrete notch carries the content a long way in each of twenty-five glide frames,
/// while a held gesture spends half its frames delivering a delta that moves the content thirty
/// pixels.
///
/// The ceilings sit just above each — 0.97 and 0.55 — so a frame that had been damaging part of the
/// surface and starts damaging all of it fails, and a phase that narrows damage passes.
pub(crate) fn damage(gesture: Gesture) -> Criterion {
    Criterion {
        name: match gesture {
            Gesture::Wheel => "LIST-damage-wheel",
            Gesture::Touchpad => "LIST-damage-touchpad",
        },
        subject: "the share of the surface one drawn frame of the gesture damaged",
        baseline: "one, which is the whole surface",
        allowed: Allowed::Under {
            most: match gesture {
                Gesture::Wheel => 0.97,
                Gesture::Touchpad => 0.55,
            },
        },
        advice: "A scrolling frame is damaging more of the surface than it did. Since a fraction \
                 cannot exceed one, the only way this moves upward is a frame that had been \
                 damaging part of the surface and is now damaging all of it: look for a full-damage \
                 frame being raised on the scroll path.",
    }
}

/// Fragments whose geometry was recomputed, per frame the gesture drew.
///
/// A count, so again the same on every machine. It is the number the deliverable names as
/// `fragments_rebuilt` per tick, and it is the one that says whether a scroll moved the document or
/// recomputed it: a virtualised list carried one row-height per frame ought to rebuild the rows that
/// crossed the boundary, not the rows that stayed still.
///
/// **7.52 per frame under the wheel and 7.06 under the touchpad**, identically on every run, against
/// thirty-nine rows realised — so about a fifth of the port, which is what a frame that recycled a
/// row or two and left the rest alone looks like. The ceiling of twelve is comfortably above both
/// and comfortably below the thirty-nine a frame rebuilding the whole port would report.
pub(crate) fn rebuilds(gesture: Gesture) -> Criterion {
    Criterion {
        name: match gesture {
            Gesture::Wheel => "LIST-rebuilds-wheel",
            Gesture::Touchpad => "LIST-rebuilds-touchpad",
        },
        subject: "fragments rebuilt per frame the gesture drew",
        allowed: Allowed::Under { most: 12.0 },
        baseline: "what a frame that moved the content rather than recomputing it rebuilds",
        advice: "A scrolling frame is recomputing more fragment geometry than it was. The other \
                 numbers in this run say which kind of frame changed: if the damage fraction is \
                 unchanged and only this moved, the frames that used to translate the content are \
                 rebuilding it.",
    }
}

/// The glide's slope per realised box, against a full repaint's slope over the same rows.
///
/// The recorded glide baseline, stated dimensionlessly. Both slopes are least-squares fits over
/// four port heights that realise between them an eightfold range of rows, taken in one process on
/// the same four documents, so the machine divides out.
///
/// A two-sided band rather than a ceiling, and it is the one gate here that has one: unlike
/// locality and virtualisation, there is no reason a glide *should* trend to zero against a
/// repaint. It is a proportion between two real pieces of work, and either half moving is worth
/// being told about — a glide that got dearer, or a repaint that got cheaper and left the glide
/// where it was.
///
/// # What the recorded value says
///
/// **0.9128–0.9193 over three runs.** One frame of a wheel glide over a virtualised list costs
/// about nine tenths of what repainting the same realised rows costs. That is the number the scroll
/// half of the compositor programme is argued against, and it is worth reading twice: carrying rows
/// past a port is very nearly as expensive as drawing them again from their styles. The advisory
/// lines say the same thing in units — 28 µs per realised row per drawn frame against 30 µs for the
/// repaint.
///
/// The band is twelve per cent, which is four times the spread of the three runs it was recorded
/// from and still narrow enough that a change moving either half by a fifth fails.
pub(crate) const GLIDE: Criterion = Criterion {
    name: "LIST-glide",
    subject: "one drawn frame of a wheel glide over a hundred thousand rows, per realised row",
    baseline: "a full repaint of the same realised rows, per realised row",
    allowed: Allowed::Near {
        recorded: 0.92,
        tolerance: 0.12,
    },
    advice: "The proportion between what carrying rows past a port costs and what repainting them \
             costs has moved. Both slopes are in the advisory lines above: the one that moved is \
             the one to look at.",
};

/// Frames of a scroll that declared the whole surface damaged.
///
/// Zero, and it is a gate rather than an observation. A scroll moves the content under a port, and
/// what that damages is the port — never the chrome around it, never the whole window. A single
/// full-damage frame on the scroll path is the one failure a *fraction* cannot report on its own:
/// a mean over twenty-five frames absorbs one of them and stays where it was.
///
/// This is also why the fraction above is not gated with a ceiling of one. A share of a surface
/// cannot exceed one, so a ceiling at one is a comparison nothing can fail — the assertion that
/// cannot fail, which is worse than no assertion. The two gates together say the thing worth
/// saying: no frame damaged everything, and the frames that damaged something damaged no more of it
/// than they did when the number was recorded.
pub(crate) const FULL_FRAMES: Criterion = Criterion {
    name: "LIST-full-frames",
    subject: "frames of one gesture that declared the whole surface damaged",
    baseline: "zero, which is how many a scroll under a port is entitled to raise",
    allowed: Allowed::Under { most: 0.0 },
    advice: "Something on the scroll path is raising full damage. It is not the scrolled content: \
             that damages the port. Look for a stage that gives up on a damage set and calls \
             `set_full` — a rectangle count past `MAX_DAMAGE`, a transform it could not bound, a \
             cache miss answered by repainting everything.",
};
