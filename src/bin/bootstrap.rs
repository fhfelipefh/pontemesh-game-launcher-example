use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use pontemesh_sdk_core::integrity::sha256_hex;
use rand::distr::{Alphanumeric, SampleString};
use reqwest::{
    blocking::{multipart, Client, Response},
    header::{COOKIE, SET_COOKIE},
};
use serde_json::{json, Value};

const ORIGIN: &str = "http://127.0.0.1:8080";
const USERNAME: &str = "admin";
const BUCKET: &str = "game-updates";
const GENERATED_PACKAGE_SIZE: usize = 8 * 1024 * 1024;
const ADMIN_PASSWORD_PATH: &str = "runtime/bootstrap-admin-password";

fn main() -> Result<(), String> {
    let client = Client::builder()
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    wait_for_origin(&client)?;
    let admin_password = load_or_create_admin_password()?;
    let status = json_response(client.get(format!("{ORIGIN}/api/setup/status")).send())?;
    if status["setupRequired"].as_bool().unwrap_or(false) {
        complete_setup(&client, &admin_password)?;
    }
    let admin_cookie = login(&client, &admin_password)?;
    create_bucket(&client, &admin_cookie)?;
    enable_lan_distribution(&client, &admin_cookie)?;
    publish_release(&client, &admin_cookie)?;
    let token = create_downloader(&client, &admin_cookie)?;
    write_secret(
        Path::new("launcher.toml"),
        &format!(
            "origin_url = \"{ORIGIN}\"\napplication_token = \"{token}\"\nrelease_bucket = \"{BUCKET}\"\nrelease_manifest_key = \"releases/stable.json\"\ninstall_directory = \"runtime/installed-game\"\n"
        ),
    )
    .map_err(|error| error.to_string())?;
    println!("Demo Origin, release, and downloader credential are ready.");
    println!("Run: cargo run");
    Ok(())
}

fn wait_for_origin(client: &Client) -> Result<(), String> {
    for _ in 0..60 {
        if client
            .get(format!("{ORIGIN}/api/setup/status"))
            .send()
            .map(|response| response.status().is_success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err("Origin did not become ready within 60 seconds".to_string())
}

fn complete_setup(client: &Client, admin_password: &str) -> Result<(), String> {
    let output = Command::new("docker")
        .args([
            "compose",
            "exec",
            "-T",
            "origin",
            "cat",
            "/var/pontemesh_home/secrets/initialAdminToken",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let unlock = checked(
        client
            .post(format!("{ORIGIN}/api/setup/unlock"))
            .json(&json!({ "token": token }))
            .send(),
    )?;
    let cookie = response_cookie(&unlock)?;
    checked(
        client
            .post(format!("{ORIGIN}/api/setup/complete"))
            .header(COOKIE, cookie)
            .json(&json!({
                "instanceName": "Game Launcher Demo",
                "role": "origin",
                "adminUsername": USERNAME,
                "adminPassword": admin_password,
                "httpPort": 8080,
                "internalStoragePath": "/var/pontemesh_home/storage"
            }))
            .send(),
    )?;
    Ok(())
}

fn login(client: &Client, admin_password: &str) -> Result<String, String> {
    let response = checked(
        client
            .post(format!("{ORIGIN}/api/auth/login"))
            .json(&json!({ "username": USERNAME, "password": admin_password }))
            .send(),
    )?;
    response_cookie(&response)
}

fn load_or_create_admin_password() -> Result<String, String> {
    let path = PathBuf::from(ADMIN_PASSWORD_PATH);
    if path.exists() {
        return fs::read_to_string(path)
            .map(|value| value.trim().to_owned())
            .map_err(|error| error.to_string());
    }
    let password = Alphanumeric.sample_string(&mut rand::rng(), 48);
    write_secret(&path, &password)?;
    Ok(password)
}

fn write_secret(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    file.write_all(contents.as_bytes())
        .and_then(|_| file.flush())
        .map_err(|error| error.to_string())
}

fn create_bucket(client: &Client, cookie: &str) -> Result<(), String> {
    let response = client
        .post(format!("{ORIGIN}/api/admin/buckets"))
        .header(COOKIE, cookie)
        .json(&json!({ "name": BUCKET }))
        .send()
        .map_err(|error| error.to_string())?;
    if response.status().is_success() || response.status().as_u16() == 400 {
        Ok(())
    } else {
        Err(response_error(response))
    }
}

fn enable_lan_distribution(client: &Client, cookie: &str) -> Result<(), String> {
    let mut policy = json_response(
        client
            .get(format!("{ORIGIN}/api/admin/buckets/{BUCKET}/policy"))
            .header(COOKIE, cookie)
            .send(),
    )?;
    policy["allowReplicaEdge"] = json!(true);
    policy["allowPeerSharing"] = json!(true);
    policy["sourceSelectionStrategy"] = json!("PEER_FIRST");
    policy["fragmentSizeBytes"] = json!(64 * 1024);
    checked(
        client
            .put(format!("{ORIGIN}/api/admin/buckets/{BUCKET}/policy"))
            .header(COOKIE, cookie)
            .json(&policy)
            .send(),
    )?;
    Ok(())
}

fn publish_release(client: &Client, cookie: &str) -> Result<(), String> {
    let generated_package = generate_game_package()?;
    let sources = [
        (generated_package.path(), "game/game-update.pak", 10_u32),
        (
            Path::new("sample-content/default-settings.json"),
            "game/default-settings.json",
            20,
        ),
        (Path::new("sample-content/news.txt"), "game/news.txt", 30),
    ];
    let mut files = Vec::new();
    for (source, destination, order) in sources {
        let bytes = fs::read(source).map_err(|error| error.to_string())?;
        let key = format!("releases/1.0.0/{destination}");
        upload(client, cookie, source, &key)?;
        files.push(json!({
            "bucket": BUCKET,
            "key": key,
            "path": destination,
            "sizeBytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "order": order
        }));
    }
    let descriptor = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 1,
        "product": "pontemesh-demo-game",
        "version": "1.0.0",
        "files": files
    }))
    .map_err(|error| error.to_string())?;
    let temporary = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    fs::write(temporary.path(), descriptor).map_err(|error| error.to_string())?;
    upload(client, cookie, temporary.path(), "releases/stable.json")
}

fn generate_game_package() -> Result<tempfile::NamedTempFile, String> {
    let mut package = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let block = (0..64 * 1024)
        .map(|index| ((index * 31 + 17) % 251) as u8)
        .collect::<Vec<_>>();
    for _ in 0..GENERATED_PACKAGE_SIZE / block.len() {
        package
            .write_all(&block)
            .map_err(|error| error.to_string())?;
    }
    package.flush().map_err(|error| error.to_string())?;
    Ok(package)
}

fn upload(client: &Client, cookie: &str, path: &Path, key: &str) -> Result<(), String> {
    let form = multipart::Form::new()
        .text("key", key.to_string())
        .file("file", path)
        .map_err(|error| error.to_string())?;
    checked(
        client
            .post(format!("{ORIGIN}/api/admin/buckets/{BUCKET}/objects"))
            .header(COOKIE, cookie)
            .multipart(form)
            .send(),
    )?;
    Ok(())
}

fn create_downloader(client: &Client, cookie: &str) -> Result<String, String> {
    let response = json_response(
        client
            .post(format!("{ORIGIN}/api/admin/application-credentials"))
            .header(COOKIE, cookie)
            .json(&json!({ "name": "game-launcher-demo", "preset": "downloader" }))
            .send(),
    )?;
    response["token"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Server did not return the application token".to_string())
}

fn json_response(response: Result<Response, reqwest::Error>) -> Result<Value, String> {
    checked(response)?.json().map_err(|error| error.to_string())
}

fn checked(response: Result<Response, reqwest::Error>) -> Result<Response, String> {
    let response = response.map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(response_error(response))
    }
}

fn response_cookie(response: &Response) -> Result<String, String> {
    response
        .headers()
        .get(SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Server did not return a session cookie".to_string())
}

fn response_error(response: Response) -> String {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    format!("Server returned {status}: {body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_package_is_large_and_deterministic() {
        let first = generate_game_package().expect("first package should be generated");
        let second = generate_game_package().expect("second package should be generated");
        let first_bytes = fs::read(first.path()).expect("first package should be readable");
        let second_bytes = fs::read(second.path()).expect("second package should be readable");

        assert_eq!(first_bytes.len(), GENERATED_PACKAGE_SIZE);
        assert_eq!(sha256_hex(&first_bytes), sha256_hex(&second_bytes));
    }
}
