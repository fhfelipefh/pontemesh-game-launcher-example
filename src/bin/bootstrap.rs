use std::{fs, path::Path, process::Command, thread, time::Duration};

use pontemesh_sdk_core::integrity::sha256_hex;
use reqwest::{
    blocking::{multipart, Client, Response},
    header::{COOKIE, SET_COOKIE},
};
use serde_json::{json, Value};

const ORIGIN: &str = "http://127.0.0.1:8080";
const USERNAME: &str = "admin";
const PASSWORD: &str = "pontemesh-demo";
const BUCKET: &str = "game-updates";

fn main() -> Result<(), String> {
    let client = Client::builder()
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    wait_for_origin(&client)?;
    let status = json_response(client.get(format!("{ORIGIN}/api/setup/status")).send())?;
    if status["setupRequired"].as_bool().unwrap_or(false) {
        complete_setup(&client)?;
    }
    let admin_cookie = login(&client)?;
    create_bucket(&client, &admin_cookie)?;
    enable_lan_distribution(&client, &admin_cookie)?;
    publish_release(&client, &admin_cookie)?;
    let token = create_downloader(&client, &admin_cookie)?;
    fs::write(
        "launcher.toml",
        format!(
            "origin_url = \"{ORIGIN}\"\napplication_token = \"{token}\"\nrelease_bucket = \"{BUCKET}\"\nrelease_manifest_key = \"releases/stable.json\"\ninstall_directory = \"installed-game\"\n"
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

fn complete_setup(client: &Client) -> Result<(), String> {
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
                "adminPassword": PASSWORD,
                "httpPort": 8080,
                "internalStoragePath": "/var/pontemesh_home/storage"
            }))
            .send(),
    )?;
    Ok(())
}

fn login(client: &Client) -> Result<String, String> {
    let response = checked(
        client
            .post(format!("{ORIGIN}/api/auth/login"))
            .json(&json!({ "username": USERNAME, "password": PASSWORD }))
            .send(),
    )?;
    response_cookie(&response)
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
    policy["fragmentSizeBytes"] = json!(1024);
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
    let sources = [
        (
            "sample-content/game-update.pak",
            "game/game-update.pak",
            10_u32,
        ),
        (
            "sample-content/default-settings.json",
            "game/default-settings.json",
            20,
        ),
        ("sample-content/news.txt", "game/news.txt", 30),
    ];
    let mut files = Vec::new();
    for (source, destination, order) in sources {
        let bytes = fs::read(source).map_err(|error| error.to_string())?;
        let key = format!("releases/1.0.0/{destination}");
        upload(client, cookie, Path::new(source), &key)?;
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
