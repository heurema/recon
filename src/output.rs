use crate::model::{Briefing, Section, SourceResult};

/// Serialize briefing to JSON (pretty-printed).
pub fn render_json(briefing: &Briefing) -> String {
    serde_json::to_string_pretty(briefing).expect("Briefing is always serializable")
}

/// Render briefing as human-readable markdown with content fencing.
pub fn render_text(briefing: &Briefing) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Recon Briefing\n\nGenerated: {}  Duration: {}ms  Partial: {}\n\n",
        briefing.generated_at.format("%Y-%m-%dT%H:%M:%SZ"),
        briefing.duration_ms,
        briefing.partial,
    ));

    out.push_str(&format!(
        "## Summary\n\nTotal: {}  OK: {}  Failed: {}  Timed out: {}\n\n",
        briefing.summary.sources_total,
        briefing.summary.sources_ok,
        briefing.summary.sources_failed,
        briefing.summary.sources_timed_out,
    ));

    for section in &briefing.sections {
        out.push_str(&render_section(section));
    }

    out
}

fn render_section(section: &Section) -> String {
    let mut out = String::new();
    out.push_str(&format!("## {}\n\n", section.title));
    for source in &section.sources {
        out.push_str(&render_source(source));
    }
    out
}

/// #4: XML-escape source id and data to prevent injection
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_source(source: &SourceResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "### {} — status: {}  duration: {}ms\n\n",
        xml_escape(&source.id), source.status, source.duration_ms
    ));

    if let Some(err) = &source.error {
        out.push_str(&format!(
            "> **Error** ({}): {}\n\n",
            xml_escape(&err.error_type), xml_escape(&err.message)
        ));
    }

    let data_str = match &source.data {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };

    // #4: escape source id in XML attribute, sanitize data for tag injection only
    let escaped_id = xml_escape(&source.id);
    // Only neutralize closing tag injection — keep data readable
    let safe_data = data_str.replace("</external_data>", "&lt;/external_data&gt;");

    if !data_str.is_empty() {
        out.push_str(&format!(
            "<external_data source=\"{}\" trust=\"untrusted\">\n{}\n</external_data>\n\n",
            escaped_id, safe_data
        ));
    } else {
        out.push_str(&format!(
            "<external_data source=\"{}\" trust=\"untrusted\">\n</external_data>\n\n",
            escaped_id
        ));
    }

    out
}
