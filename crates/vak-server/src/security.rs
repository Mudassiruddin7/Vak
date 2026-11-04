use axum::extract::Request;
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// Headers every response carries. The page loads nothing from other origins except the Supabase project
/// it signs in against, and never lets another site frame it.
pub async fn headers(supabase_origin: Option<String>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let connect = match &supabase_origin {
        Some(o) => format!("'self' {o}"),
        None => "'self'".to_string(),
    };
    let csp = format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; \
         font-src 'self'; connect-src {connect}; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'"
    );
    let h = res.headers_mut();
    for (name, value) in [
        (header::CONTENT_SECURITY_POLICY, csp.as_str()),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::HeaderName::from_static("permissions-policy"), "microphone=(self), camera=(), geolocation=(), payment=()"),
        (header::HeaderName::from_static("cross-origin-opener-policy"), "same-origin"),
        (header::STRICT_TRANSPORT_SECURITY, "max-age=31536000; includeSubDomains"),
    ] {
        if let Ok(v) = HeaderValue::from_str(value) {
            h.insert(name, v);
        }
    }
    res
}
