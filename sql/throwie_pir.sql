CREATE TABLE throwie_pir (
    sensor_id MACADDR,
    timestamp TIMESTAMP NOT NULL,
    version VARCHAR(16) NOT NULL,
    level BOOLEAN NOT NULL,
    PRIMARY KEY (sensor_id, timestamp)
);
SELECT create_hypertable('throwie_pir', 'timestamp');
--SELECT add_retention_policy('throwie_pir', INTERVAL '100 hours');