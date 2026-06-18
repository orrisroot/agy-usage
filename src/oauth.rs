use rand::distr::Alphanumeric;
use rand::{RngExt, rng};
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use url::Url;

use crate::config::{StoredTokens, save_account_tokens, save_global_config};
use crate::google_api::{
    OAUTH_AUTH_URL, OAUTH_CLIENT_ID, exchange_code_for_tokens, get_user_email, resolve_project_id,
};

fn generate_state() -> String {
    rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}

async fn send_response(
    socket: &mut tokio::net::TcpStream,
    status_code: u16,
    content: &str,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        status_text,
        content.len(),
        content
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

pub struct LoginOptions {
    pub no_browser: bool,
    pub manual: bool,
    pub port: Option<u16>,
}

pub async fn run_login(options: LoginOptions) -> Result<String, Box<dyn std::error::Error>> {
    let port = if let Some(p) = options.port {
        p
    } else {
        // Find a free port
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };

    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);
    let state = generate_state();

    let mut auth_url = Url::parse(OAUTH_AUTH_URL)?;
    auth_url.query_pairs_mut()
        .append_pair("client_id", OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair(
            "scope",
            "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email",
        )
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", &state);

    let auth_url_str = auth_url.to_string();

    if options.manual {
        println!("\n=== MANUAL LOGIN MODE ===");
        println!("1. Open the following URL in your browser and log in:");
        println!("{}\n", auth_url_str);
        println!("2. Once logged in, you will be redirected to a page that may fail to load.");
        println!("3. Copy the ENTIRE redirect URL from your browser's address bar.");
        print!("Paste the full redirect URL here: ");
        io::stdout().flush()?;

        let mut input_url = String::new();
        io::stdin().read_line(&mut input_url)?;
        let input_url = input_url.trim();

        let parsed_url = Url::parse(input_url)?;
        let query_params: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();

        if let Some(err) = query_params.get("error") {
            return Err(format!("Login failed with error from Google: {}", err).into());
        }

        let code = query_params
            .get("code")
            .ok_or("Authorization code missing from pasted URL")?;
        let returned_state = query_params
            .get("state")
            .ok_or("State parameter missing from pasted URL")?;

        if returned_state != &state {
            return Err("State mismatch. Security verification failed.".into());
        }

        return complete_login(code, &redirect_uri).await;
    }

    // Standard loopback server flow
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    println!("\nStarting local server for login callback...");

    if options.no_browser {
        println!("Please open the following URL in your browser to log in:");
        println!("{}\n", auth_url_str);
    } else {
        println!("Opening browser for Google login...");
        if let Err(_) = open::that(&auth_url_str) {
            println!("Could not open browser automatically.");
            println!("Please visit this URL to log in:");
            println!("{}\n", auth_url_str);
        } else {
            println!("If the browser did not open, visit this URL:");
            println!("{}\n", auth_url_str);
        }
    }

    println!("Waiting for authentication (2 minute timeout)...");

    let timeout_duration = Duration::from_secs(120);
    let code_result = tokio::time::timeout(timeout_duration, async {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            match socket.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let request_str = String::from_utf8_lossy(&buf[..n]);
                    if let Some(first_line) = request_str.lines().next() {
                        let parts: Vec<&str> = first_line.split_whitespace().collect();
                        if parts.len() >= 2 && parts[0] == "GET" {
                            let path_and_query = parts[1];
                            if let Ok(url) = Url::parse(&format!("http://127.0.0.1:{}", path_and_query)) {
                                if url.path() == "/callback" {
                                    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

                                    if let Some(err) = params.get("error") {
                                        let html = format!("<html><body><h1>Login Failed</h1><p>Google returned error: {}</p></body></html>", err);
                                        let _ = send_response(&mut socket, 400, &html).await;
                                        return Err(format!("Google error: {}", err));
                                    }

                                    let code = match params.get("code") {
                                        Some(c) => c.clone(),
                                        None => {
                                            let html = "<html><body><h1>Invalid Request</h1><p>Missing auth code.</p></body></html>";
                                            let _ = send_response(&mut socket, 400, html).await;
                                            return Err("Missing code".to_string());
                                        }
                                    };

                                    let returned_state = match params.get("state") {
                                        Some(s) => s.clone(),
                                        None => {
                                            let html = "<html><body><h1>Invalid Request</h1><p>Missing state parameter.</p></body></html>";
                                            let _ = send_response(&mut socket, 400, html).await;
                                            return Err("Missing state".to_string());
                                        }
                                    };

                                    if returned_state != state {
                                        let html = "<html><body><h1>Invalid Request</h1><p>State mismatch.</p></body></html>";
                                        let _ = send_response(&mut socket, 400, html).await;
                                        return Err("State mismatch".to_string());
                                    }

                                    // Process login
                                    match complete_login(&code, &redirect_uri).await {
                                        Ok(email) => {
                                            let html = format!(
                                                "<html><body style=\"font-family: system-ui; padding: 40px; text-align: center;\">\
                                                 <h1>Login Successful!</h1>\
                                                 <p>You are now logged in as <strong>{}</strong>.</p>\
                                                 <p>You can close this window and return to the terminal.</p>\
                                                 </body></html>",
                                                email
                                            );
                                            let _ = send_response(&mut socket, 200, &html).await;
                                            return Ok(email);
                                        }
                                        Err(err) => {
                                            let html = format!("<html><body><h1>Login Failed</h1><p>Token processing error: {}</p></body></html>", err);
                                            let _ = send_response(&mut socket, 500, &html).await;
                                            return Err(err.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }).await;

    match code_result {
        Ok(Ok(email)) => Ok(email),
        Ok(Err(err)) => Err(err.into()),
        Err(_) => Err("Login timed out after 2 minutes".into()),
    }
}

async fn complete_login(
    code: &str,
    redirect_uri: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    println!("Exchanging authorization code for tokens...");
    let token_resp = exchange_code_for_tokens(code, redirect_uri).await?;

    println!("Retrieving user email...");
    let email = get_user_email(&token_resp.access_token).await?;

    println!("Resolving project ID...");
    let project_id = resolve_project_id(&token_resp.access_token, None).await;

    let now = chrono::Utc::now().timestamp_millis() as u64;
    let tokens = StoredTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        expires_at: now + token_resp.expires_in * 1000,
        email: email.clone(),
        project_id,
    };

    save_account_tokens(&email, &tokens)?;

    // Set as active account
    let mut global = crate::config::load_global_config();
    global.active_email = Some(email.clone());
    save_global_config(&global)?;

    Ok(email)
}
