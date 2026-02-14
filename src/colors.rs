//! S-57 COLOUR attribute handling
//!
//! COLOUR Attribute values from S-57 specification:
//! ID  | Meaning  | INT 1 | S-4
//! ----|----------|-------|--------
//! 1   | white    | IP 11.1 | 450.2-3
//! 2   | black    |       |
//! 3   | red      | IP 11.2 | 450.2-3
//! 4   | green    | IP 11.3 | 450.2-3
//! 5   | blue     | IP 11.4 | 450.2-3
//! 6   | yellow   | IP 11.6 | 450.2-3
//! 7   | grey     |       |
//! 8   | brown    |       |
//! 9   | amber    | IP 11.8 | 450.2-3
//! 10  | violet   | IP 11.5 | 450.2-3
//! 11  | orange   | IP 11.7 | 450.2-3
//! 12  | magenta  |       |
//! 13  | pink     |       |

use serde_json::{Map, Value};

/// S-57 COLOUR attribute enum
///
/// Represents standardized colors used in maritime navigation features
/// such as lights, buoys, and beacons. The integer discriminants match
/// the S-57 specification exactly.
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    White = 1,
    Black = 2,
    Red = 3,
    Green = 4,
    Blue = 5,
    Yellow = 6,
    Grey = 7,
    Brown = 8,
    Amber = 9,
    Violet = 10,
    Orange = 11,
    Magenta = 12,
    Pink = 13,
}

impl Colour {
    /// Parse a COLOUR value from S-57 integer code
    pub fn from_i64(val: i64) -> Option<Self> {
        match val {
            1 => Some(Colour::White),
            2 => Some(Colour::Black),
            3 => Some(Colour::Red),
            4 => Some(Colour::Green),
            5 => Some(Colour::Blue),
            6 => Some(Colour::Yellow),
            7 => Some(Colour::Grey),
            8 => Some(Colour::Brown),
            9 => Some(Colour::Amber),
            10 => Some(Colour::Violet),
            11 => Some(Colour::Orange),
            12 => Some(Colour::Magenta),
            13 => Some(Colour::Pink),
            _ => None,
        }
    }
}

/// Parse COLOUR attribute from S-57 feature attributes
///
/// The COLOUR attribute in S-57 can be a single value or an array of values
/// (for multi-colored features like sector lights). This function handles both
/// cases and returns a list of parsed colors.
///
/// S-57 COLOUR can appear as:
/// - Integer: 6
/// - Integer array: [6]
/// - String (from StringList): "6"
/// - String array (from StringList): ["6"]
///
/// # Arguments
/// * `attrs` - Feature attribute map from S-57 extraction
///
/// # Returns
/// Vector of parsed Colour enums. Empty if COLOUR attribute is missing or invalid.
pub fn parse_colours(attrs: &Map<String, Value>) -> Vec<Colour> {
    attrs
        .get("COLOUR")
        .and_then(|v| {
            // Handle array (most common case in S-57)
            if let Some(arr) = v.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|elem| {
                            // Try as integer
                            if let Some(i) = elem.as_i64() {
                                Colour::from_i64(i)
                            }
                            // Try as string and parse
                            else if let Some(s) = elem.as_str() {
                                s.parse::<i64>().ok().and_then(Colour::from_i64)
                            } else {
                                None
                            }
                        })
                        .collect(),
                )
            }
            // Handle single integer value
            else if let Some(i) = v.as_i64() {
                Colour::from_i64(i).map(|c| vec![c])
            }
            // Handle single string value
            else if let Some(s) = v.as_str() {
                s.parse::<i64>().ok().and_then(Colour::from_i64).map(|c| vec![c])
            } else {
                None
            }
        })
        .unwrap_or_default()
}
