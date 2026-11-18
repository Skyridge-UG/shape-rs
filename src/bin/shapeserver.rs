//! `shapeserver` — the Shape solver HTTP API.
//!
//! ```text
//! GET  /healthz           liveness + build info
//! GET  /profiles          device profiles known to the catalog
//! POST /shape             solve: {profile?, seed0?, seed1?, site} -> header bundle JSON
//! ```
//!
//! The solve path is synchronous CPU work (microseconds) — no shared state,
//! so the default actix worker pool is plenty. Loopback bind; put a router in
//! front for exposure (mirrors the reese84/datadome server contracts).

use std::time::Instant;

use actix_web::{App, HttpResponse, HttpServer, Responder, post, web};
use serde::Deserialize;
use shape::encode::encode_smoke;
use shape::synth::Synthesizer;

/// Solve request. `profile` names a catalog device; seeds come from the
/// caller's session (defaults: time-derived, unique per request).
#[derive(Debug, Deserialize)]
struct SolveRequest {
    /// Device profile name; omitted = the built-in win11-chrome default.
    #[serde(default)]
    profile: Option<String>,

    /// Site id for the header prefix (`x-<site>-a`). Default `example`.
    #[serde(default = "default_site")]
    site: String,

    /// Explicit payload seeds; omitted = derived from the clock.
    #[serde(default)]
    seed0: Option<u32>,
    #[serde(default)]
    seed1: Option<u32>,
}

fn default_site() -> String {
    "example".to_owned()
}

/// Known profiles. The catalog grows with the device corpus; names are
/// stable API.
fn profile_by_name(name: Option<&str>) -> Result<shape::DeviceProfile, String> {
    match name {
        None | Some("win11-chrome") => Ok(shape::DeviceProfile::win11_chrome()),
        Some(other) => Err(format!("unknown profile `{other}`; known: win11-chrome")),
    }
}

async fn healthcheck() -> impl Responder {
    web::Json(serde_json::json!({
        "status": "ok",
        "service": "shapeserver",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn profiles() -> impl Responder {
    web::Json(serde_json::json!({
        "profiles": ["win11-chrome"],
    }))
}

#[post("/shape")]
async fn solve(body: web::Json<SolveRequest>) -> impl Responder {
    let started = Instant::now();
    let request = body.into_inner();

    let profile = match profile_by_name(request.profile.as_deref()) {
        Ok(p) => p,
        Err(message) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "code": "unknown_profile", "message": message },
            }));
        }
    };

    // Seeds: caller-provided or time-derived (unique per request, stable
    // within it so retries with the same body reproduce).
    let now = Instant::now();
    let seed0 = request
        .seed0
        .unwrap_or_else(|| (now.elapsed().as_nanos() & 0xffff_ffff) as u32);
    let seed1 = request.seed1.unwrap_or(0x0061_0062); // reference token `ab`

    let synthesizer = Synthesizer::new(profile);
    let signals = match synthesizer.bootstrap(u64::from(seed0)) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "code": "synthesis", "message": e.to_string() },
            }));
        }
    };

    let prefix = format!("x-{}-", request.site);
    match encode_smoke(&prefix, &signals, [seed0, seed1]) {
        Ok(bundle) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            HttpResponse::Ok()
                .insert_header(("X-Solver-Backend", "shape"))
                .insert_header(("X-Solver-Latency-Ms", latency_ms.to_string()))
                .json(serde_json::json!({
                    "site": request.site,
                    "seeds": [seed0, seed1],
                    "headers": bundle,
                }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": { "code": "encode", "message": e.to_string() },
        })),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = std::env::var("SHAPE_RS_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("SHAPE_RS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9110);
    eprintln!("shapeserver listening on http://{host}:{port}");
    HttpServer::new(|| {
        App::new()
            .route("/healthz", web::get().to(healthcheck))
            .route("/profiles", web::get().to(profiles))
            .service(solve)
    })
    .workers(2)
    .bind((host, port))?
    .run()
    .await
}
