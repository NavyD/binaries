DROP TABLE IF EXISTS updated_info;

CREATE TABLE IF NOT EXISTS `updated_info` (
    `id` INTEGER PRIMARY KEY AUTOINCREMENT,
    `name` VARCHAR(20) NOT NULL,
    `version` VARCHAR(20) NOT NULL,
    `create_time` datetime NOT NULL,
    `updated_time` datetime NOT NULL
);

INSERT INTO
    `updated_info`
VALUES
    (
        1,
        'btm',
        'v0.6.0',
        '2020-06-17 20:10:23',
        '2020-06-17 20:10:23'
    ),
    (
        2,
        'tldr',
        'v0.2.0',
        '2020-06-17 20:10:23',
        '2020-07-17 21:10:23'
    ),
    (
        3,
        'btm',
        'v0.7.0',
        '2021-06-17 20:10:23',
        '2021-06-17 20:10:23'
    );