use anyhow::{anyhow, Result};

/// Constants for XPT format parsing
mod constants {
    /// Standard XPT record size in bytes
    pub const RECORD_SIZE: usize = 80;
    /// Length of a name string record in bytes
    pub const NAME_STRING_RECORD_LENGTH: usize = 140;
    /// Minimum length for numeric variables (IBM 360 floating point)
    pub const MIN_NUMERIC_LENGTH: usize = 8;
    /// Minimum length for character variables
    pub const MIN_CHARACTER_LENGTH: usize = 1;
}

/// Represents a parsed XPT dataset
#[derive(Debug, Clone)]
pub struct XPTDataset {
    pub title: String,
    pub created_date: Option<String>,
    pub modified_date: Option<String>,
    pub variables: Vec<XPTVariable>,
    pub rows: Vec<XPTRow>,
}

/// Represents a variable (column) in an XPT dataset
#[derive(Debug, Clone)]
pub struct XPTVariable {
    pub name: String,
    pub label: String,
    pub var_type: VariableType,
    pub length: usize,
}

/// Variable type (numeric or character)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableType {
    Numeric,
    Character,
}

/// Represents a row of data
#[derive(Debug, Clone)]
pub struct XPTRow {
    pub values: Vec<String>,
}

/// Internal structure for parsing name string records
struct NameStringRecord {
    var_type: u16,
    length: u16,
    name: String,
    label: String,
    #[allow(dead_code)]
    format: String, // Parsed but not currently used - may be useful for future enhancements
    position: u16,
}

/// Parser for SAS XPORT Version 5 transport files
pub struct XPTParser;

impl XPTParser {
    /// Parses a SAS XPORT Version 5 transport file
    ///
    /// The XPT format uses a fixed 80-byte record structure. The file contains:
    /// 1. Header records identifying sections (NAMESTR for variable metadata, OBS for observations)
    /// 2. Variable metadata records (140 bytes each) describing column names, types, and formats
    /// 3. Observation data records containing the actual row data
    pub fn parse(data: &[u8], suggested_filename: Option<&str>) -> Result<XPTDataset> {
        if data.len() < constants::RECORD_SIZE {
            return Err(anyhow!("File too small to be a valid XPT file"));
        }

        // Locate the two critical header sections
        let namestr_header = b"HEADER RECORD*******NAMESTR HEADER RECORD!!!!!!!";
        let obs_header = b"HEADER RECORD*******OBS     HEADER RECORD!!!!!!!";

        let namestr_header_pos = find_bytes(data, namestr_header)
            .ok_or_else(|| anyhow!("NAMESTR header not found"))?;
        let obs_header_pos = find_bytes(data, obs_header)
            .ok_or_else(|| anyhow!("OBS header not found"))?;

        // Extract the variable metadata block between headers
        let name_str_block_start = align_to_record_boundary(namestr_header_pos + namestr_header.len());
        let name_str_block_end = obs_header_pos;

        if name_str_block_end <= name_str_block_start {
            return Err(anyhow!("Invalid header positions"));
        }

        let name_string_block = &data[name_str_block_start..name_str_block_end];

        if name_string_block.len() < constants::NAME_STRING_RECORD_LENGTH {
            return Err(anyhow!("Name string block too small"));
        }

        // Each variable metadata record is exactly 140 bytes
        let record_count = name_string_block.len() / constants::NAME_STRING_RECORD_LENGTH;
        if record_count == 0 {
            return Err(anyhow!("The file does not include variable metadata"));
        }

        // Parse all variable metadata records
        let mut name_records = Vec::with_capacity(record_count);
        for i in 0..record_count {
            let start = i * constants::NAME_STRING_RECORD_LENGTH;
            let end = start + constants::NAME_STRING_RECORD_LENGTH;
            if end <= name_string_block.len() {
                if let Some(record) = Self::parse_name_string(&name_string_block[start..end]) {
                    name_records.push(record);
                }
            }
        }

        if name_records.is_empty() {
            return Err(anyhow!("Variable descriptors could not be parsed"));
        }

        let dataset_title = Self::infer_dataset_title(data, suggested_filename);
        let created_date = Self::infer_date(data, b"DATECREATED");
        let modified_date = Self::infer_date(data, b"DATEMODIFIED");

        // Sort variables by their position field, using index as fallback if position is 0
        let mut ordered_records: Vec<(usize, NameStringRecord)> = name_records
            .into_iter()
            .enumerate()
            .collect();
        ordered_records.sort_by(|(lhs_idx, lhs), (rhs_idx, rhs)| {
            let lhs_order = if lhs.position > 0 {
                lhs.position as usize
            } else {
                lhs_idx + 1
            };
            let rhs_order = if rhs.position > 0 {
                rhs.position as usize
            } else {
                rhs_idx + 1
            };
            lhs_order.cmp(&rhs_order).then_with(|| lhs_idx.cmp(rhs_idx))
        });

        let variables: Vec<XPTVariable> = ordered_records
            .into_iter()
            .enumerate()
            .map(|(index, (_, record))| {
                let base_name = if record.name.is_empty() {
                    format!("VAR{}", index + 1)
                } else {
                    record.name
                };
                let label = if record.label.is_empty() {
                    base_name.clone()
                } else {
                    record.label
                };
                let var_type = if record.var_type == 1 {
                    VariableType::Numeric
                } else {
                    VariableType::Character
                };
                let length = if var_type == VariableType::Numeric {
                    record.length.max(constants::MIN_NUMERIC_LENGTH as u16) as usize
                } else {
                    record.length.max(constants::MIN_CHARACTER_LENGTH as u16) as usize
                };

                XPTVariable {
                    name: base_name,
                    label,
                    var_type,
                    length,
                }
            })
            .collect();

        // Extract observation data, starting after the OBS header
        let obs_data_start = align_to_record_boundary(obs_header_pos + obs_header.len());
        let raw_observation_bytes = &data[obs_data_start..];

        // Calculate the total storage width needed for all variables
        let storage_width: usize = variables.iter().map(|v| v.length).sum();
        if storage_width == 0 {
            return Err(anyhow!("Variables have zero length"));
        }

        // XPT format may pad rows to 8-byte boundaries for alignment
        let row_width_candidates = vec![
            storage_width,
            ((storage_width as f64 / 8.0).ceil() as usize) * 8,
        ];

        // Determine the actual row width
        let mut resolved_row_width: Option<usize> = None;
        let mut observation_bytes = raw_observation_bytes;

        for candidate in row_width_candidates {
            let remainder = raw_observation_bytes.len() % candidate;
            if remainder == 0 {
                resolved_row_width = Some(candidate);
                break;
            }

            // Check if remainder is just padding (null bytes or spaces)
            if remainder > 0 {
                let filler_start = raw_observation_bytes.len() - remainder;
                let filler_bytes = &raw_observation_bytes[filler_start..];
                if filler_bytes.iter().all(|&b| b == 0x00 || b == 0x20) {
                    resolved_row_width = Some(candidate);
                    observation_bytes = &raw_observation_bytes[..filler_start];
                    break;
                }
            }
        }

        let row_width = resolved_row_width
            .ok_or_else(|| anyhow!("Unable to determine observation width"))?;
        if observation_bytes.len() < row_width {
            return Err(anyhow!("Observation data too small"));
        }

        let observation_count = observation_bytes.len() / row_width;
        let mut rows = Vec::with_capacity(observation_count);

        for row_idx in 0..observation_count {
            let row_start = row_idx * row_width;
            let row_end = row_start + storage_width;
            if row_end > observation_bytes.len() {
                break;
            }

            let row_data = &observation_bytes[row_start..row_end];
            let mut row_values = Vec::with_capacity(variables.len());
            let mut offset = 0;

            for variable in &variables {
                if offset + variable.length > row_data.len() {
                    break;
                }
                let cell_data = &row_data[offset..offset + variable.length];
                let value = Self::parse_cell(cell_data, variable);
                row_values.push(value);
                offset += variable.length;
            }

            if row_values.len() == variables.len() {
                rows.push(XPTRow { values: row_values });
            }

            // Skip padding if present
            let filler = row_width - storage_width;
            if filler > 0 {
                // Already handled by row_width calculation
            }
        }

        Ok(XPTDataset {
            title: dataset_title,
            created_date,
            modified_date,
            variables,
            rows,
        })
    }

    /// Parses a name string record (140 bytes)
    fn parse_name_string(data: &[u8]) -> Option<NameStringRecord> {
        if data.len() < constants::NAME_STRING_RECORD_LENGTH {
            return None;
        }

        let var_type = u16::from_be_bytes([data[0], data[1]]);
        let length = u16::from_be_bytes([data[4], data[5]]);
        let position = u16::from_be_bytes([data[6], data[7]]);
        let name = ascii_string(data, 8, 8);
        let label = ascii_string(data, 16, 40);
        let format = ascii_string(data, 56, 8);

        Some(NameStringRecord {
            var_type,
            length,
            name,
            label,
            format,
            position,
        })
    }

    /// Parses a cell value based on variable type
    fn parse_cell(data: &[u8], variable: &XPTVariable) -> String {
        match variable.var_type {
            VariableType::Character => {
                ascii_string_trimmed(data)
            }
            VariableType::Numeric => {
                Self::parse_numeric_value(data)
            }
        }
    }

    /// Decodes an IBM System/360 floating-point number from 8 bytes
    ///
    /// The IBM 360 floating-point format (also used by SAS) uses hexadecimal base:
    /// - Byte 0: Sign bit (bit 7) + 7-bit exponent (bits 0-6)
    /// - Bytes 1-7: 56-bit fraction (mantissa)
    ///
    /// Formula: value = sign × (fraction / 2^56) × 16^exponent
    fn parse_numeric_value(data: &[u8]) -> String {
        if data.len() < 8 {
            return String::new();
        }

        let bytes = &data[0..8];

        // Check for zero value (all bytes are zero)
        if bytes.iter().all(|&b| b == 0) {
            return "0".to_string();
        }

        // Check for missing value marker (SAS convention: 0x2E in first byte)
        if bytes[0] == 0x2E {
            return String::new();
        }

        // Extract sign bit (most significant bit of first byte)
        let sign = (bytes[0] & 0x80) != 0;

        // Extract exponent (lower 7 bits of first byte), adjust for excess-64 encoding
        let exponent = (bytes[0] & 0x7F) as i32 - 64;

        // Extract 56-bit fraction from remaining 7 bytes
        let mut fraction: u64 = 0;
        for &byte in bytes.iter().skip(1) {
            fraction = (fraction << 8) | u64::from(byte);
        }

        // Handle zero fraction case
        if fraction == 0 {
            return if sign { "-0".to_string() } else { "0".to_string() };
        }

        // Convert fraction to decimal: divide by 2^56 to normalize
        let mut value = fraction as f64 / (1u64 << 56) as f64;

        // Apply hexadecimal exponent: multiply by 16^exponent
        value *= 16.0_f64.powi(exponent);

        // Apply sign
        if sign {
            value *= -1.0;
        }

        // Format and return, handling non-finite values (infinity, NaN)
        if value.is_finite() {
            // Format with up to 6 decimal places, removing trailing zeros
            let formatted = format!("{:.6}", value);
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            if trimmed.is_empty() {
                "0".to_string()
            } else {
                trimmed.to_string()
            }
        } else {
            String::new()
        }
    }

    /// Infers the dataset title from the file
    fn infer_dataset_title(data: &[u8], fallback: Option<&str>) -> String {
        let member_marker = b"MEMBER  NAME";
        if let Some(pos) = find_bytes(data, member_marker) {
            let start = pos + member_marker.len();
            let limit = (start + 80).min(data.len());
            if let Ok(text) = String::from_utf8(data[start..limit].to_vec()) {
                let components: Vec<&str> = text
                    .split(|c: char| c == ' ' || c == '\0')
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(name) = components.first() {
                    return name.trim().to_string();
                }
            }
        }

        if let Some(fallback) = fallback {
            if let Some(name) = std::path::Path::new(fallback)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                return name.to_string();
            }
        }

        "XPT Dataset".to_string()
    }

    /// Infers a date from the file using a marker
    fn infer_date(data: &[u8], marker: &[u8]) -> Option<String> {
        let pos = find_bytes(data, marker)?;
        let start = pos + marker.len();
        let limit = (start + 32).min(data.len());
        let slice = &data[start..limit];
        if let Ok(text) = String::from_utf8(slice.to_vec()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        None
    }
}

/// Helper function to find a byte sequence in data
fn find_bytes(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

/// Aligns a byte index to the nearest 80-byte record boundary
fn align_to_record_boundary(index: usize) -> usize {
    let remainder = index % constants::RECORD_SIZE;
    if remainder == 0 {
        index
    } else {
        index + (constants::RECORD_SIZE - remainder)
    }
}

/// Extracts an ASCII string from data at a specific offset and length
fn ascii_string(data: &[u8], offset: usize, length: usize) -> String {
    if offset >= data.len() || offset + length > data.len() {
        return String::new();
    }
    let slice = &data[offset..offset + length];
    String::from_utf8_lossy(slice)
        .trim_end_matches(|c: char| c.is_whitespace() || c == '\0')
        .to_string()
}

/// Extracts an ASCII string from data and trims whitespace
fn ascii_string_trimmed(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .trim_end_matches(|c: char| c.is_whitespace() || c == '\0')
        .to_string()
}

