//! The content of a scroll container mirrored under a compositor scroll node.

mod support;

use iceblink::Command;
use support::compositor::Recorded;
use support::{Element, Harness};
use zgui_space::spatial::PropertyOwner;

/// A port 100 pixels tall over ten rows of 30, with a sticky heading above them.
fn list() -> Harness {
    let mut rows = vec![Element::new("heading")];
    rows.extend((0..10).map(|_| Element::new("row")));
    Harness::sized(
        Element::new("root").children(vec![Element::new("port").children(rows)]),
        "root { display: block; width: 200px; height: 200px }
         port { display: block; width: 200px; height: 100px; overflow: hidden }
         heading { display: block; height: 10px; background: green;
                   position: sticky; top: 0px }
         row { display: block; height: 30px; background: blue }",
        200.0,
        200.0,
    )
}

/// Returns the device rectangles of every quad the last frame drew with `fill`.
fn drawn(output: &Recorded, fill: &str) -> Vec<String> {
    let Some(drawing) = output.output.drawing() else {
        return Vec::new();
    };
    drawing
        .lines()
        .filter(|line| line.contains(fill))
        .map(|line| {
            let space = line.split("space=").nth(1).unwrap_or("");
            let bounds = line.split("bounds=").nth(1).unwrap_or("");
            format!("{} {}", bounds.split(')').next().unwrap_or(""), space)
        })
        .collect()
}

#[test]
fn a_scroll_writes_the_scroll_node_and_nothing_else() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = list();
    // A port that only code scrolls takes its content space at its first scroll.
    harness.scroll_to("port", 0.0, 20.0);
    output.frame_everything(&mut harness);
    let port = harness.box_of("port");
    let sync = harness
        .painter
        .compositor()
        .expect("attached by the first frame");
    let scroll = sync
        .scroll_node(&harness.scene, port)
        .expect("the port's content is mirrored under a scroll node");
    assert_eq!(sync.scroll_container(scroll), Some(port));

    harness.scroll_to("port", 0.0, 40.0);
    let written = output.frame(&mut harness);

    assert_eq!(
        (written.commit.property_writes, written.report.table_writes),
        (1, 1),
        "{written:?}"
    );
    assert_eq!(
        written.report.recorded + written.report.offsets,
        0,
        "the sticky heading is held by the compositor and moves no chunk: {written:?}"
    );
}

#[test]
fn rows_are_drawn_where_the_scroll_offset_puts_them() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = list();
    output.frame_everything(&mut harness);

    harness.scroll_to("port", 0.0, 40.0);
    output.frame_everything(&mut harness);
    if output.output.drawing().is_none() {
        return;
    }
    let first = drawn(&output, "premul[0, 0, 1, 1]");

    assert!(
        first
            .iter()
            .any(|row| row.ends_with("translate[0, -40]") && row.starts_with("rect(0, 40,")),
        "the second row is drawn at the top of the port: {first:#?}"
    );
}

#[test]
fn an_offset_past_the_scroll_range_is_drawn_where_it_puts_the_rows() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = list();
    output.frame_everything(&mut harness);

    // Ten rows of 30 and the heading scroll 210 pixels at most in a port of 100.
    harness.scroll_to("port", 0.0, 250.0);
    output.frame_everything(&mut harness);
    if output.output.drawing().is_none() {
        return;
    }

    let rows = drawn(&output, "premul[0, 0, 1, 1]");
    assert!(
        rows.iter().any(|row| row.ends_with("translate[0, -250]")),
        "the rows are drawn 250 pixels up: {rows:#?}"
    );
}

#[test]
fn a_sticky_heading_stays_at_the_top_of_its_port() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = list();
    output.frame_everything(&mut harness);

    let mut places = Vec::new();
    for offset in [20.0, 60.0] {
        harness.scroll_to("port", 0.0, offset);
        output.frame_everything(&mut harness);
        if output.output.drawing().is_none() {
            return;
        }
        places.push(drawn(&output, "premul[0, 0.5"));
    }

    // The port's top edge is the device's, so a heading held there is drawn at its own place
    // through no translation at all.
    for (offset, place) in [20, 60].iter().zip(&places) {
        assert_eq!(
            place.len(),
            1,
            "one heading at offset {offset}: {places:#?}"
        );
        assert_eq!(
            place[0], "rect(0, 0, 200, 10 ",
            "the heading is drawn at the top of the port at offset {offset}: {places:#?}"
        );
    }
}

#[test]
fn an_adopted_offset_is_acknowledged_instead_of_written() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = list();
    harness.scroll_to("port", 0.0, 20.0);
    output.frame_everything(&mut harness);
    let port = harness.box_of("port");
    let sync = harness
        .painter
        .compositor()
        .expect("attached by the first frame");
    let scroll = sync
        .scroll_node(&harness.scene, port)
        .expect("the port's content is mirrored under a scroll node");

    sync.adopt_scroll(scroll, 7);
    harness.scroll_to("port", 0.0, 40.0);
    let transaction = output.output.transaction();
    let sync = harness.painter.compositor().expect("attached");
    sync.open(transaction);
    harness.paint();
    let parts = harness
        .painter
        .compositor()
        .expect("attached")
        .close(&harness.store, &mut harness.scene)
        .expect("opened above")
        .into_parts();

    let scrolls: Vec<&Command> = parts
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                Command::ScrollTo(..) | Command::AcknowledgeScroll { .. }
            )
        })
        .collect();
    assert!(
        matches!(
            scrolls[..],
            [Command::AcknowledgeScroll { id, serial: 7, offset: [0.0, 40.0] }] if *id == scroll
        ),
        "{scrolls:?}"
    );
}

/// The device translation of the first quad the last frame drew with `fill`, as the transcript
/// writes it. The transcript leaves the identity out.
fn space_of(output: &Recorded, fill: &str) -> Option<String> {
    let drawing = output.output.drawing()?;
    let line = drawing.lines().find(|line| line.contains(fill))?;
    Some(
        line.split("space=")
            .nth(1)
            .unwrap_or("translate[0, 0]")
            .to_owned(),
    )
}

#[test]
fn a_fixed_box_is_drawn_through_the_matrix_zgui_resolves_for_it() {
    let Some(mut output) = Recorded::new([200, 200]) else {
        return;
    };
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("outer").children(vec![
            Element::new("spacer"),
            Element::new("port").children(vec![
                Element::new("row"),
                Element::new("card").children(vec![Element::new("fix")]),
                Element::new("row"),
                Element::new("row"),
            ]),
        ])]),
        "root { display: block; width: 200px; height: 200px }
         outer { display: block; width: 200px; height: 200px; overflow: hidden }
         spacer { display: block; height: 30px }
         port { display: block; width: 200px; height: 100px; overflow: hidden }
         row { display: block; height: 60px; background: gray }
         card { display: block; height: 40px; transform: translateX(10px) }
         fix { display: block; position: fixed; top: 5px; left: 5px; width: 20px; height: 20px;
               background: red }",
        200.0,
        200.0,
    );
    harness.scroll_to("outer", 0.0, 20.0);
    harness.scroll_to("port", 0.0, 30.0);
    output.frame_everything(&mut harness);
    if output.output.drawing().is_none() {
        return;
    }
    let drawn = space_of(&output, "premul[1, 0, 0, 1]").expect("the fixed box is drawn");

    let fix = harness.box_of("fix");
    let space = harness
        .scene
        .spatial
        .of(PropertyOwner::of(fix))
        .expect("a fixed box establishes a coordinate system");
    let matrix = harness
        .scene
        .spatial
        .resolve(space)
        .expect("the fixed box's coordinate system resolves");
    let [x, y, ..] = matrix.columns[3];
    assert_eq!(drawn, format!("translate[{x}, {y}]"));
}
