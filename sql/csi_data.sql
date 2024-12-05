CREATE TABLE csi_data (
    sensor_id VARCHAR,
    timestamp TIMESTAMP NOT NULL,
    imag BYTEA,
    real BYTEA,
    sequence_identifier INTEGER,
    antenna SMALLINT,
    rssi SMALLINT,
    noise_floor SMALLINT,
    interval INTEGER,
    PRIMARY KEY (sensor_id, timestamp),

    CONSTRAINT check_imag_size CHECK (octet_length(imag) = 64),
    CONSTRAINT check_real_size CHECK (octet_length(real) = 64)
);
SELECT create_hypertable('csi_data', 'timestamp');