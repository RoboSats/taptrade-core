use axum::{
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;


#[derive(Deserialize, Serialize, Debug)]
struct OrderRequest {
    robohash_base91: String,
    amount_satoshi: u64,
    order_type: String,
    bond_ratio: u8,
}
// Handler function to process the received data
async fn receive_order(Json(order): Json<OrderRequest>) {
    // Print the received data to the console
    println!("Received order: {:?}", order);
    
    // Access individual fields
    let robohash = &order.robohash_base91;
    let amount = order.amount_satoshi;
    let order_type = &order.order_type;
    let bond_ratio = order.bond_ratio;

    // Process the data as needed
    // For example, you can log the data, save it to a database, etc.
    println!("Robohash: {}", robohash);
    println!("Amount (satoshi): {}", amount);
    println!("Order type: {}", order_type);
    println!("Bond ratio: {}", bond_ratio);

    // Example of further processing
    if order_type == "buy" {
        println!("Processing a buy order...");
        // Add your buy order logic here
    } else if order_type == "sell" {
        println!("Processing a sell order...");
        // Add your sell order logic here
    }
}

#[tokio::main]
pub async fn webserver() {
    // Build our application with a single route
    let app = Router::new().route("/receive-order", post(receive_order));

    // Run the server on localhost:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    // axum::Server::bind(&addr)
    //     .serve(app.into_make_service())
    //     .await
    //     .unwrap();
    let tcp = TcpListener::bind(&addr).await.unwrap();
    axum::serve(tcp, app).await.unwrap();

}


// // use axum

// #[get("/")] 
// fn index() -> &'static str {
//     "Hello, world!"
// }

// #[launch]
// pub fn webserver() -> Rocket<build> {
//     rocket::build().mount("/", routes![index])
// }

// // serde to parse json
// // https://www.youtube.com/watch?v=md-ecvXBGzI  BDK + Webserver video
// // https://github.com/tokio-rs/axum