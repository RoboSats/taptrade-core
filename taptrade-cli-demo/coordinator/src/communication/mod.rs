use rocket::{get, launch, build, Rocket};

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
pub fn webserver() -> Rocket<build> {
    rocket::build().mount("/", routes![index])
}