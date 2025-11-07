#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Context, Result};
use readstat::parser::Parser;
use readstat::{value::Value, ReadStatError, VarType};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path, sync::Mutex};
use tauri::Manager;

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
    let mut datasets = Vec::new();

    let mut parser = Parser::new()
        .with_context(|| "Unable to initialise ReadStat parser for SAS XPORT files")?;

    #[derive(Default)]
    struct ParsingState {
        dataset_name: Option<String>,
        dataset_label: Option<String>,
        fields: Vec<FieldMetadata>,
        rows: Vec<BTreeMap<String, serde_json::Value>>,
    }

    let state = Mutex::new(ParsingState::default());

    parser.set_metadata_handler({
        let state = &state;
        move |metadata| {
            let mut state = state.lock().expect("metadata mutex poisoned");
            if let Some(name) = metadata.table_name() {
                state.dataset_name = Some(name.to_string());
            }
            state.dataset_label = metadata.file_label().map(|label| label.to_string());
            Ok(())
        }
    });

    parser.set_variable_handler({
        let state = &state;
        move |variable| {
            let mut state = state.lock().expect("variable mutex poisoned");
            let field = FieldMetadata {
                name: variable.name().to_string(),
                label: variable.label().map(|label| label.to_string()),
                kind: match variable.var_type() {
                    VarType::String => "Character".to_string(),
                    _ => "Numeric".to_string(),
                },
            };
            state.fields.push(field);
            Ok(())
        }
    });

    parser.set_record_handler({
        let state = &state;
        move |record| {
            let mut state = state.lock().expect("record mutex poisoned");
            let mut row = BTreeMap::new();
            for (index, field) in state.fields.iter().enumerate() {
                let value = record.value(index);
                row.insert(field.name.clone(), convert_value(&value)?);
            }
            state.rows.push(row);
            Ok(())
        }
    });

    parser
        .read_path(path)
        .with_context(|| format!("Unable to parse SAS XPORT file: {}", path.display()))?;

    let state = state
        .into_inner()
        .expect("failed to retrieve parsing state after completion");

    datasets.push(DatasetSummary {
        name: state.dataset_name.unwrap_or_else(|| "Dataset".to_string()),
        label: state.dataset_label,
        observation_count: state.rows.len(),
        fields: state.fields,
        rows: state.rows,
    });

    Ok(XptFilePayload {
        path: path.display().to_string(),
        datasets,
    })
}

fn convert_value(value: &Value) -> Result<serde_json::Value, ReadStatError> {
    if value.is_system_missing() || value.is_user_missing() {
        return Ok(serde_json::Value::Null);
    }

    match value.var_type() {
        VarType::String => Ok(serde_json::Value::String(
            value.string().unwrap_or_default().trim_end().to_string(),
        )),
        VarType::Numeric => {
            let number = value.double()?;
            if number.is_finite() {
                serde_json::Number::from_f64(number)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| ReadStatError::InvalidData("Invalid numeric value".into()))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        _ => Ok(serde_json::Value::Null),
    }
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
