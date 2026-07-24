pub fn generate_uuid() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // UUID v4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant RFC 4122
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub fn sanitize_body(body: &str) -> String {
    // Try parsing as JSON first
    if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(body) {
        sanitize_json_value(&mut val);
        return serde_json::to_string(&val).unwrap_or_else(|_| body.to_string());
    }

    // Try parsing as form urlencoded
    if body.contains('=') && !body.contains('{') && !body.contains('[') {
        let mut params = Vec::new();
        let mut modified = false;
        for (k, v) in url::form_urlencoded::parse(body.as_bytes()) {
            if k == "access_token" || k == "refresh_token" || k == "client_secret" || k == "code" {
                params.push((k.into_owned(), "***".to_string()));
                modified = true;
            } else {
                params.push((k.into_owned(), v.into_owned()));
            }
        }
        if modified {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in params {
                serializer.append_pair(&k, &v);
            }
            return serializer.finish();
        }
    }

    body.to_string()
}

fn sanitize_json_value(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let is_sensitive = k == "access_token"
                    || k == "accessToken"
                    || k == "refresh_token"
                    || k == "refreshToken"
                    || k == "client_secret"
                    || k == "clientSecret"
                    || k == "code";
                if is_sensitive {
                    *v = serde_json::Value::String("***".to_string());
                } else {
                    sanitize_json_value(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                sanitize_json_value(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_json() {
        let input = r#"{"access_token":"secret123","refresh_token":"ref456","other":"public"}"#;
        let sanitized = sanitize_body(input);
        let parsed: serde_json::Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed["access_token"], "***");
        assert_eq!(parsed["refresh_token"], "***");
        assert_eq!(parsed["other"], "public");
    }

    #[test]
    fn test_sanitize_nested_json() {
        let input = r#"{"nested":{"accessToken":"secret123"},"list":[{"code":"mycode"}]}"#;
        let sanitized = sanitize_body(input);
        let parsed: serde_json::Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed["nested"]["accessToken"], "***");
        assert_eq!(parsed["list"][0]["code"], "***");
    }

    #[test]
    fn test_sanitize_form_urlencoded() {
        let input = "code=mycode&client_id=123&client_secret=secret&other=val";
        let sanitized = sanitize_body(input);
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(sanitized.as_bytes())
                .into_owned()
                .collect();
        assert_eq!(params.get("code").unwrap(), "***");
        assert_eq!(params.get("client_id").unwrap(), "123");
        assert_eq!(params.get("client_secret").unwrap(), "***");
        assert_eq!(params.get("other").unwrap(), "val");
    }
}
