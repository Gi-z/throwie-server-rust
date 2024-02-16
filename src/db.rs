use influxdb::{Client, WriteQuery};

async fn write_batch(client: &Client, readings: Vec<WriteQuery>) {
    let write_result = client
        .query(readings)
        .await;
    // println!("{}", write_result.unwrap());
    assert!(write_result.is_ok(), "Write result was not okay");
}

