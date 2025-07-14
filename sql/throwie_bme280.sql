CREATE TABLE throwie_bme280 (
    sensor_id MACADDR,
    timestamp TIMESTAMP NOT NULL,
    version VARCHAR(16) NOT NULL,
    temperature float4 NOT NULL,
    humidity float4 NOT NULL,
    pressure float4 NOT NULL,
    PRIMARY KEY (sensor_id, timestamp)
);
SELECT create_hypertable('throwie_bme280', 'timestamp');
--SELECT add_retention_policy('throwie_bme280', INTERVAL '100 hours');