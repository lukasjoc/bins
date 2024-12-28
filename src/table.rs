use serde::Serialize;

pub(crate) trait TableRow<'a>: Serialize {
    /// Get the names for the columns for a table line.
    fn columns(&self) -> Option<Vec<String>> {
        if let Ok(value) = serde_json::to_value(self) {
            if let Some(keys) = value.as_object().map(|o| o.keys()) {
                return keys
                    .map(|k| k.to_string().to_uppercase())
                    .collect::<Vec<_>>()
                    .into();
            }
        }
        return None;
    }
    /// Get the raw cell data for each line based on the columns specified in the type.
    fn cells(&self) -> Option<Vec<String>> {
        if let Ok(value) = serde_json::to_value(self) {
            if let Some(values) = value.as_object().map(|o| o.values()) {
                return values
                    .map(
                        |v| match (v.as_str(), v.as_bool(), v.as_u64(), v.as_i64()) {
                            (Some(str_v), None, None, None) => str_v.to_string(),
                            (None, Some(bool_v), None, None) => bool_v.to_string(),
                            (None, None, Some(u64_v), None) => u64_v.to_string(),
                            (None, None, None, Some(i64_v)) => i64_v.to_string(),
                            (None, None, Some(_), Some(i64_v)) => i64_v.to_string(),
                            (Some(_), Some(_), Some(_), Some(_)) => unreachable!(),
                            _ => unimplemented!(),
                        },
                    )
                    .collect::<Vec<_>>()
                    .into();
            }
        }
        return None;
    }
}

#[inline]
pub(crate) fn render_ansi<'a, RowType: TableRow<'a>>(rows: &[RowType], column_spacing: usize) {
    if rows.len() <= 0 {
        return;
    }
    let cols = rows.first().unwrap().columns().unwrap_or_default();
    let mut column_widths: Vec<usize> = cols.iter().map(|name| name.len()).collect();
    let cells: Vec<String> = rows
        .iter()
        .flat_map(|line| line.cells().unwrap_or_default())
        .collect();
    for column_index in 0..cols.len() {
        for i in (column_index..cells.len()).step_by(cols.len()) {
            if let Some(content) = cells.get(i) {
                if content.len() > column_widths[column_index] {
                    column_widths[column_index] = content.len();
                }
            };
        }
    }

    for column_index in 0..cols.len() {
        let content = &cols[column_index];
        let width = column_widths[column_index];
        let abs_width = width.saturating_sub(content.len()) + column_spacing;
        print!("{}{}", content, " ".repeat(abs_width));
    }
    print!("\n");

    let mut column_index = 0;
    for content in cells {
        let max_width = column_widths[column_index];
        let abs_width = max_width.saturating_sub(content.len()) + column_spacing;
        print!("{}{}", content, " ".repeat(abs_width));
        if column_index == cols.len() - 1 {
            column_index = 0;
            print!("\n");
        } else {
            column_index += 1;
        }
    }
    print!("\n");
}
