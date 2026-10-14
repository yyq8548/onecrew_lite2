//! Native browser launch; Library startup belongs to the shared host.
use std::path::Path;
pub(crate) async fn open(
    executable: &Path,
    workbench_url: &str,
    no_browser: bool,
) -> Result<(), String> {
    let library_url = onecrew_local_service::library_workspace_url(executable).await?;
    crate::print_json(&serde_json::json!({
        "workbench_url": workbench_url,
        "library_url": library_url,
    }));
    if !no_browser {
        crate::open_browser(workbench_url.to_owned()).await?;
        crate::open_browser(library_url).await?;
    }
    Ok(())
}
