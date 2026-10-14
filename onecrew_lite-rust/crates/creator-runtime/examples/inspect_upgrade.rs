//! Read-only local upgrade preflight; never starts the service or migrates data.
fn main() {
    let path = std::env::args_os().nth(1).expect("database path required");
    let status = onecrew_creator_runtime::CreatorRuntime::inspect_database(std::path::Path::new(&path));
    println!("{}", serde_json::to_string_pretty(&status).unwrap());
}
