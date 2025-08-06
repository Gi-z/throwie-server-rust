CREATE TABLE throwie_sensor (
    id MACADDR,
    mode SMALLINT, -- 0 = sensor, 1 = injector
    location VARCHAR(20),

    PRIMARY KEY (id)
);