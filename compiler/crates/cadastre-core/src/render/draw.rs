//! Drawing helpers of the SVG drawn by the compiler (`layout: elk`, `render traversal`): the
//! shapes of the kinds, lines of text, icons, arrow markers.

use std::fmt::Write;

use super::{Line, Scene, html, icon_class};
use crate::registry::ResourceId;

/// Width of a character, as a share of the size of the text: Helvetica, on average.
pub(super) const CHAR_WIDTH: f64 = 0.6;

pub(super) const LINE_HEIGHT: f64 = 1.3;

/// Space between the text of a node and its border, horizontally and vertically.
pub(super) const PADDING: (f64, f64) = (10.0, 7.0);

/// Height of the ellipses closing a cylinder.
pub(super) const CYLINDER_CAP: f64 = 6.0;

/// The kind whose icon a node or a frame shows (`icons` render option).
pub(super) fn kind_of(scene: &Scene, id: ResourceId) -> Option<String> {
    icon_class(scene, id).and_then(|class| class.strip_prefix("kind_").map(str::to_owned))
}

/// The size of lines of text, estimated from their number of characters.
pub(super) fn text_size(lines: &[String], size: f64) -> (f64, f64) {
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    (longest * size * CHAR_WIDTH, lines.len().max(1) as f64 * size * LINE_HEIGHT)
}

/// The size of a shape holding a box of text: an ellipse and a hexagon are wider, a cylinder
/// higher (its caps).
pub(super) fn outer_size(shape: &str, width: f64, height: f64) -> (f64, f64) {
    match shape {
        "ellipse" => (width * 1.3, height * 1.3),
        "hexagon" => (width + height, height),
        "cylinder" => (width, height + 2.0 * CYLINDER_CAP),
        "note" => (width + 8.0, height),
        _ => (width, height),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn shape(
    out: &mut String,
    shape: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: &str,
    stroke: &str,
    line: Line,
) {
    let paint = format!("fill=\"{fill}\" stroke=\"{stroke}\"{}", dash(line));
    let _ = match shape {
        "rounded" => {
            writeln!(out, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"8\" {paint}/>")
        }
        "ellipse" => writeln!(
            out,
            "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"{:.1}\" ry=\"{:.1}\" {paint}/>",
            x + w / 2.0,
            y + h / 2.0,
            w / 2.0,
            h / 2.0
        ),
        "hexagon" => {
            let i = h / 2.0;
            writeln!(
                out,
                "<polygon points=\"{:.1},{y:.1} {:.1},{y:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {x:.1},{:.1}\" {paint}/>",
                x + i,
                x + w - i,
                x + w,
                y + h / 2.0,
                x + w - i,
                y + h,
                x + i,
                y + h,
                y + h / 2.0
            )
        }
        "cylinder" => {
            let (rx, c) = (w / 2.0, CYLINDER_CAP);
            writeln!(
                out,
                "<path d=\"M{x:.1},{:.1} a{rx:.1},{c} 0 0,0 {w:.1},0 v{:.1} a{rx:.1},{c} 0 0,1 {:.1},0 z\" {paint}/>\n\
                 <path d=\"M{x:.1},{:.1} a{rx:.1},{c} 0 0,1 {w:.1},0 a{rx:.1},{c} 0 0,1 {:.1},0\" {paint}/>",
                y + c,
                h - 2.0 * c,
                -w,
                y + c,
                -w
            )
        }
        "note" => {
            let f = 8.0;
            writeln!(
                out,
                "<path d=\"M{x:.1},{y:.1} h{:.1} l{f},{f} v{:.1} h{:.1} z M{:.1},{y:.1} v{f} h{f}\" {paint}/>",
                w - f,
                h - f,
                -w,
                x + w - f
            )
        }
        _ => writeln!(out, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" {paint}/>"),
    };
}

/// Lines of text centered on `x`, the first baseline at `top`.
pub(super) fn text(out: &mut String, lines: &[String], x: f64, top: f64, size: f64, color: &str) {
    for (i, line) in lines.iter().enumerate() {
        let _ = writeln!(
            out,
            "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"{size}\" text-anchor=\"middle\" fill=\"{color}\">{}</text>",
            top + i as f64 * size * LINE_HEIGHT,
            html(line),
        );
    }
}

pub(super) fn icon_use(out: &mut String, kind: &str, x: f64, y: f64, side: f64, color: &str) {
    let _ = writeln!(
        out,
        "<use xlink:href=\"#icon-{kind}\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{side}\" height=\"{side}\" style=\"color:{color}\"/>"
    );
}

/// The marker of the arrows of a color, filled or hollow (`trigger`).
pub(super) fn marker_id(markers: &mut Vec<(String, bool)>, color: &str, hollow: bool) -> String {
    let index = markers.iter().position(|(c, h)| c == color && *h == hollow).unwrap_or_else(|| {
        markers.push((color.to_owned(), hollow));
        markers.len() - 1
    });
    format!("arrow{index}")
}

pub(super) fn dash(line: Line) -> &'static str {
    match line {
        Line::Solid => "",
        Line::Dashed => " stroke-dasharray=\"5 3\"",
        Line::Dotted => " stroke-dasharray=\"2 2\"",
    }
}
