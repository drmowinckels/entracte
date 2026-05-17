/// Renderer-side error reporter. The web error boundary calls this
/// from `componentDidCatch` so the crash lands in the same rotating
/// log file as everything else, instead of vanishing into the webview
/// devtools console where no end user ever looks.
#[tauri::command]
pub fn report_renderer_error(
    message: String,
    stack: Option<String>,
    component_stack: Option<String>,
) {
    log::error!(
        "renderer: {} | stack={} | component_stack={}",
        message,
        stack.as_deref().unwrap_or("<none>"),
        component_stack.as_deref().unwrap_or("<none>")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_renderer_error_accepts_full_args() {
        report_renderer_error(
            "boom".to_string(),
            Some("at foo (a.js:1)".to_string()),
            Some("in <App/>".to_string()),
        );
    }

    #[test]
    fn report_renderer_error_accepts_missing_optionals() {
        report_renderer_error("boom".to_string(), None, None);
    }
}
