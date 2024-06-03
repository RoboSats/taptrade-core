use rocket::{get, launch, build, Rocket};

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
pub fn webserver() -> Rocket<build> {
    rocket::build().mount("/", routes![index])
}

// serde to parse json
// https://www.youtube.com/watch?v=md-ecvXBGzI  BDK + Webserver video
// https://github.com/tokio-rs/axum