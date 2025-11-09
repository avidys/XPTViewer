#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod xpt_parser;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tauri::Manager;
use xpt_parser::{XPTParser, VariableType};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldMetadata {
    name: String,
    label: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DatasetSummary {
    name: String,
    label: Option<String>,
    created_date: Option<String>,
    modified_date: Option<String>,
    observation_count: usize,
    fields: Vec<FieldMetadata>,
    rows: Vec<BTreeMap<String, serde_json::Value>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct XptFilePayload {
    path: String,
    datasets: Vec<DatasetSummary>,
}

#[tauri::command]
fn load_xpt(path: String) -> Result<XptFilePayload, String> {
    match load_xpt_impl(Path::new(&path)) {
        Ok(payload) => Ok(payload),
        Err(error) => Err(error.to_string()),
    }
}

fn load_xpt_impl(path: &Path) -> Result<XptFilePayload> {
    // Read the file
    let data = fs::read(path)
        .with_context(|| format!("Unable to read file: {}", path.display()))?;

    // Parse using our XPT parser
    let suggested_filename = path
        .file_name()
        .and_then(|n| n.to_str());
    
    let dataset = XPTParser::parse(&data, suggested_filename)
        .with_context(|| format!("Unable to parse SAS XPORT file: {}", path.display()))?;

    // Convert to the expected format
    let fields: Vec<FieldMetadata> = dataset
        .variables
        .iter()
        .map(|var| FieldMetadata {
            name: var.name.clone(),
            label: if var.label.is_empty() {
                None
            } else {
                Some(var.label.clone())
            },
            kind: match var.var_type {
                VariableType::Character => "Character".to_string(),
                VariableType::Numeric => "Numeric".to_string(),
            },
        })
        .collect();

    let rows: Vec<BTreeMap<String, serde_json::Value>> = dataset
        .rows
        .iter()
        .map(|row| {
            let mut map = BTreeMap::new();
            for (i, value) in row.values.iter().enumerate() {
                if i < fields.len() {
                    let field_name = &fields[i].name;
                    // Convert string values to appropriate JSON types
                    let json_value = if value.is_empty() {
                        serde_json::Value::Null
                    } else if fields[i].kind == "Numeric" {
                        // Try to parse as number
                        value
                            .parse::<f64>()
                            .ok()
                            .and_then(|n| serde_json::Number::from_f64(n))
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|| serde_json::Value::String(value.clone()))
                    } else {
                        serde_json::Value::String(value.clone())
                    };
                    map.insert(field_name.clone(), json_value);
                }
            }
            map
        })
        .collect();

    // Debug: log the first row to verify data structure
    if !rows.is_empty() {
        eprintln!("First row keys: {:?}", rows[0].keys().collect::<Vec<_>>());
        eprintln!("First row sample: {:?}", rows[0]);
    }
    eprintln!("Total rows: {}, Total fields: {}", rows.len(), fields.len());

    let datasets = vec![DatasetSummary {
        name: dataset.title,
        label: None, // XPT format doesn't have dataset-level labels in the same way
        created_date: dataset.created_date,
        modified_date: dataset.modified_date,
        observation_count: dataset.rows.len(),
        fields,
        rows,
    }];

    Ok(XptFilePayload {
        path: path.display().to_string(),
        datasets,
    })
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app
                    .get_window("main")
                    .ok_or_else(|| anyhow!("Missing main window"))?;
                window.set_title("XPTViewer").ok();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![load_xpt])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
