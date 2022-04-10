
DELETE FROM `updated_info`;

CREATE TABLE IF NOT EXISTS `updated_info` (
    `id` INT PRIMARY KEY NOT NULL,
    `name` TEXT NOT NULL,
    `version` TEXT NOT NULL,
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
        '2020-06-17 20:10:23'
    );