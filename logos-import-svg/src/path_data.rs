//! SVG path data parser.
//!
//! Parses the `d` attribute of SVG `<path>` elements into
//! logos-core `PathCommand` sequences.

use logos_core::{PathCommand, Point};
use logos_import_common::ImportResult;

/// Parse an SVG path data string into a sequence of PathCommands.
pub fn parse_path_data(d: &str) -> ImportResult<Vec<PathCommand>> {
    let mut commands = Vec::new();
    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut start_x: f32 = 0.0;
    let mut start_y: f32 = 0.0;

    let tokens = tokenize_path(d);
    let mut i = 0;

    while i < tokens.len() {
        let cmd = tokens[i].as_str().to_string();
        i += 1; // advance past the command token

        match cmd.as_str() {
            "M" => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                cursor_x = x;
                cursor_y = y;
                start_x = x;
                start_y = y;
                commands.push(PathCommand::MoveTo(Point::new(x, y)));
                // Subsequent coordinate pairs are implicit LineTo
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    cursor_x = x;
                    cursor_y = y;
                    commands.push(PathCommand::LineTo(Point::new(x, y)));
                }
            }
            "m" => {
                let (dx, dy) = read_pair(&tokens, &mut i)?;
                cursor_x += dx;
                cursor_y += dy;
                start_x = cursor_x;
                start_y = cursor_y;
                commands.push(PathCommand::MoveTo(Point::new(cursor_x, cursor_y)));
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (dx, dy) = read_pair(&tokens, &mut i)?;
                    cursor_x += dx;
                    cursor_y += dy;
                    commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
                }
            }
            "L" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    cursor_x = x;
                    cursor_y = y;
                    commands.push(PathCommand::LineTo(Point::new(x, y)));
                }
            }
            "l" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (dx, dy) = read_pair(&tokens, &mut i)?;
                    cursor_x += dx;
                    cursor_y += dy;
                    commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
                }
            }
            "H" => {
                let x = read_number(&tokens, &mut i)?;
                cursor_x = x;
                commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
            }
            "h" => {
                let dx = read_number(&tokens, &mut i)?;
                cursor_x += dx;
                commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
            }
            "V" => {
                let y = read_number(&tokens, &mut i)?;
                cursor_y = y;
                commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
            }
            "v" => {
                let dy = read_number(&tokens, &mut i)?;
                cursor_y += dy;
                commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
            }
            "C" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (x1, y1) = read_pair(&tokens, &mut i)?;
                    let (x2, y2) = read_pair(&tokens, &mut i)?;
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    cursor_x = x;
                    cursor_y = y;
                    commands.push(PathCommand::BezierTo {
                        cp1: Point::new(x1, y1),
                        cp2: Point::new(x2, y2),
                        end: Point::new(x, y),
                    });
                }
            }
            "c" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (dx1, dy1) = read_pair(&tokens, &mut i)?;
                    let (dx2, dy2) = read_pair(&tokens, &mut i)?;
                    let (dx, dy) = read_pair(&tokens, &mut i)?;
                    let cp1 = Point::new(cursor_x + dx1, cursor_y + dy1);
                    let cp2 = Point::new(cursor_x + dx2, cursor_y + dy2);
                    cursor_x += dx;
                    cursor_y += dy;
                    commands.push(PathCommand::BezierTo {
                        cp1,
                        cp2,
                        end: Point::new(cursor_x, cursor_y),
                    });
                }
            }
            "Q" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (cx, cy) = read_pair(&tokens, &mut i)?;
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    cursor_x = x;
                    cursor_y = y;
                    commands.push(PathCommand::QuadTo {
                        ctrl: Point::new(cx, cy),
                        end: Point::new(x, y),
                    });
                }
            }
            "q" => {
                while i < tokens.len() && is_number(&tokens[i]) {
                    let (dcx, dcy) = read_pair(&tokens, &mut i)?;
                    let (dx, dy) = read_pair(&tokens, &mut i)?;
                    let ctrl = Point::new(cursor_x + dcx, cursor_y + dcy);
                    cursor_x += dx;
                    cursor_y += dy;
                    commands.push(PathCommand::QuadTo {
                        ctrl,
                        end: Point::new(cursor_x, cursor_y),
                    });
                }
            }
            "A" | "a" => {
                // Arcs: approximate as line-to for now
                let is_rel = cmd == "a";
                // skip 5 params: rx ry x-rotation large-arc sweep
                for _ in 0..5 {
                    if i < tokens.len() && is_number(&tokens[i]) {
                        let _ = read_number(&tokens, &mut i)?;
                    }
                }
                if i < tokens.len() && is_number(&tokens[i]) {
                    let (x, y) = read_pair(&tokens, &mut i)?;
                    if is_rel {
                        cursor_x += x;
                        cursor_y += y;
                    } else {
                        cursor_x = x;
                        cursor_y = y;
                    }
                    commands.push(PathCommand::LineTo(Point::new(cursor_x, cursor_y)));
                }
            }
            "Z" | "z" => {
                commands.push(PathCommand::Close);
                cursor_x = start_x;
                cursor_y = start_y;
            }
            _ => {
                // unknown command, already advanced past it
            }
        }
    }

    Ok(commands)
}

/// Tokenize an SVG path data string into commands and numbers.
fn tokenize_path(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch.is_ascii_alphabetic() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
            i += 1;
        } else if ch == '-' && !current.is_empty() && !current.ends_with('e') && !current.ends_with('E') {
            // Negative sign starts a new number (unless after exponent)
            tokens.push(std::mem::take(&mut current));
            current.push(ch);
            i += 1;
        } else if ch == '.' && current.contains('.') {
            // Second decimal point starts a new number
            tokens.push(std::mem::take(&mut current));
            current.push(ch);
            i += 1;
        } else if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            current.push(ch);
            i += 1;
        } else if ch == ',' || ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn is_number(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
}

fn read_number(tokens: &[String], i: &mut usize) -> ImportResult<f32> {
    if *i >= tokens.len() {
        return Err(logos_import_common::ImportError::ParseError {
            offset: *i,
            message: "expected number".into(),
        });
    }
    let val = tokens[*i]
        .parse::<f32>()
        .map_err(|_| logos_import_common::ImportError::ParseError {
            offset: *i,
            message: format!("invalid number: {}", tokens[*i]),
        })?;
    *i += 1;
    Ok(val)
}

fn read_pair(tokens: &[String], i: &mut usize) -> ImportResult<(f32, f32)> {
    let x = read_number(tokens, i)?;
    let y = read_number(tokens, i)?;
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_move_line_close() {
        let cmds = parse_path_data("M 10 20 L 30 40 Z").unwrap();
        assert_eq!(cmds.len(), 3);
        assert!(matches!(cmds[0], PathCommand::MoveTo(_)));
        assert!(matches!(cmds[1], PathCommand::LineTo(_)));
        assert!(matches!(cmds[2], PathCommand::Close));
    }

    #[test]
    fn test_parse_relative_move_line() {
        let cmds = parse_path_data("m 10 20 l 5 5 l 10 0").unwrap();
        assert_eq!(cmds.len(), 3); // M + 2 L
        match &cmds[2] {
            PathCommand::LineTo(p) => {
                assert!((p.x - 25.0).abs() < 0.01);
                assert!((p.y - 25.0).abs() < 0.01);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn test_parse_cubic_bezier() {
        let cmds = parse_path_data("M 0 0 C 10 0 20 10 20 20").unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[1], PathCommand::BezierTo { .. }));
    }

    #[test]
    fn test_parse_quadratic_bezier() {
        let cmds = parse_path_data("M 0 0 Q 50 50 100 0").unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[1], PathCommand::QuadTo { .. }));
    }

    #[test]
    fn test_parse_horizontal_vertical() {
        let cmds = parse_path_data("M 0 0 H 100 V 50").unwrap();
        assert_eq!(cmds.len(), 3);
        match &cmds[1] {
            PathCommand::LineTo(p) => {
                assert!((p.x - 100.0).abs() < 0.01);
                assert!((p.y - 0.0).abs() < 0.01);
            }
            _ => panic!("expected H LineTo"),
        }
        match &cmds[2] {
            PathCommand::LineTo(p) => {
                assert!((p.x - 100.0).abs() < 0.01);
                assert!((p.y - 50.0).abs() < 0.01);
            }
            _ => panic!("expected V LineTo"),
        }
    }

    #[test]
    fn test_parse_implicit_lineto_after_move() {
        let cmds = parse_path_data("M 0 0 10 10 20 20").unwrap();
        assert_eq!(cmds.len(), 3); // M + 2 implicit L
    }

    #[test]
    fn test_tokenize_compact_path() {
        let tokens = tokenize_path("M10,20L30-40");
        assert!(tokens.len() >= 5);
    }

    #[test]
    fn test_parse_empty_path() {
        let cmds = parse_path_data("").unwrap();
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_parse_triangle() {
        let cmds = parse_path_data("M 100 10 L 40 198 L 190 78 Z").unwrap();
        assert_eq!(cmds.len(), 4);
        assert!(matches!(cmds[3], PathCommand::Close));
    }

    #[test]
    fn test_parse_relative_cubic() {
        let cmds = parse_path_data("M 0 0 c 10 0 20 10 20 20").unwrap();
        assert_eq!(cmds.len(), 2);
        match &cmds[1] {
            PathCommand::BezierTo { end, .. } => {
                assert!((end.x - 20.0).abs() < 0.01);
                assert!((end.y - 20.0).abs() < 0.01);
            }
            _ => panic!("expected BezierTo"),
        }
    }
}
