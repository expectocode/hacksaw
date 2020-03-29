use super::parse_format::{parse_format_string, Format};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "hacksaw", max_term_width = 80)]
pub(crate) struct Opt {
    #[structopt(
        short = "n",
        long = "no-guides",
        help = "Disable fighter pilot guide lines"
    )]
    pub(crate) no_guides: bool,

    #[structopt(
        short = "g",
        long = "guide-thickness",
        default_value = "1",
        help = "Thickness of fighter pilot guide lines"
    )]
    pub(crate) guide_thickness: u16,

    #[structopt(
        short = "s",
        long = "select-thickness",
        default_value = "1",
        help = "Thickness of selection box lines"
    )]
    pub(crate) select_thickness: u16,

    #[structopt(
        short = "c",
        long = "colour",
        default_value = "#7f7f7f",
        parse(try_from_str = parse_hex),
        help = "Hex colour of the lines (RGB or RGBA), '#' optional"
    )]
    pub(crate) line_colour: u32,

    #[structopt(
        short = "f",
        long = "format",
        default_value = "%g",
        parse(try_from_str = parse_format_string),
        allow_hyphen_values = true,
        help = "\
Output format. You can use:
      %x for x-coordinate,
      %y for y-coordinate,
      %w for width,
      %h for height,
      %i for selected window id,
      %g as a shorthand for %wx%h+%x+%y (X geometry),
      %% for a literal '%'.
Other %-codes will cause an error."
    )]
    pub(crate) format: Format,

    #[structopt(
        short = "r",
        long = "remove-decorations",
        default_value = "0",
        help = "Number of (nested) window manager frames to try and remove"
    )]
    pub(crate) remove_decorations: u32,
}

#[derive(Debug)]
struct ParseHexError {
    reason: String,
}

impl std::fmt::Display for ParseHexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl From<std::num::ParseIntError> for ParseHexError {
    fn from(err: std::num::ParseIntError) -> ParseHexError {
        ParseHexError {
            reason: err.to_string(),
        }
    }
}

/// Parse an HTML-color-like hex input
fn parse_hex(hex: &str) -> Result<u32, ParseHexError> {
    let hex = hex.trim_start_matches('#');
    let mut color;

    match hex.len() {
        3 | 4 => {
            color = 0x11 * u32::from_str_radix(&hex[2..3], 16)?
                + 0x11_00 * u32::from_str_radix(&hex[1..2], 16)?
                + 0x11_00_00 * u32::from_str_radix(&hex[0..1], 16)?;

            if hex.len() == 4 {
                color |= 0x11_00_00_00 * u32::from_str_radix(&hex[3..4], 16)?
            } else {
                color |= 0xFF_00_00_00;
            }
        }

        6 | 8 => {
            color = u32::from_str_radix(&hex, 16)?;

            if hex.len() == 6 {
                color |= 0xFF_00_00_00;
            }
        }

        _ => {
            return Err(ParseHexError {
                reason: "Bad hex colour".to_owned(),
            })
        }
    }

    Ok(color)
}
