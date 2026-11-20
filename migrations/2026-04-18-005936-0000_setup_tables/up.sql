-- Your SQL goes here
CREATE TABLE `categories`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`name` TEXT NOT NULL,
	`description` TEXT,
	`color` TEXT,
	`icon` TEXT,
	`user_id` TEXT,
	`created_at` TIMESTAMP NOT NULL,
	`updated_at` TIMESTAMP NOT NULL
);

CREATE TABLE `account_transfers`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`from_account_id` TEXT NOT NULL,
	`to_account_id` TEXT NOT NULL,
	`amount` BIGINT NOT NULL,
	`description` TEXT,
	`date` TIMESTAMP NOT NULL,
	`user_id` TEXT NOT NULL,
	`created_at` TIMESTAMP NOT NULL,
	`updated_at` TIMESTAMP NOT NULL
);

CREATE TABLE `transactions`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`amount` BIGINT NOT NULL,
	`description` TEXT,
	`date` TIMESTAMP NOT NULL,
	`type` TEXT NOT NULL,
	`account_id` TEXT NOT NULL,
	`transfer_id` TEXT,
	`category_id` TEXT,
	`user_id` TEXT NOT NULL,
	`created_at` TIMESTAMP NOT NULL,
	`updated_at` TIMESTAMP NOT NULL
);

CREATE TABLE `users`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`name` TEXT NOT NULL,
	`currency` TEXT,
	`email` TEXT NOT NULL,
	`password` TEXT,
	`created_at` TIMESTAMP NOT NULL,
	`updated_at` TIMESTAMP NOT NULL
);

CREATE TABLE `accounts`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`name` TEXT NOT NULL,
	`icon` TEXT,
	`balance` BIGINT NOT NULL,
	`bank_name` TEXT NOT NULL,
	`user_id` TEXT NOT NULL,
	`created_at` TIMESTAMP NOT NULL,
	`updated_at` TIMESTAMP NOT NULL
);

