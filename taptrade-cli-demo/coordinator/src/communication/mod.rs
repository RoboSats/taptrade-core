// use axum

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}
j
#[launch]
pub fn webserver() -> Rocket<build> {
    rocket::build().mount("/", routes![index])
}

// serde to parse json
// https://www.youtube.com/watch?v=md-ecvXBGzI  BDK + Webserver video
// https://github.com/tokio-rs/axum