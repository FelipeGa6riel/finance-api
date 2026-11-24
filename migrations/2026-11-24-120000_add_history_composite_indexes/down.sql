DROP INDEX IF EXISTS `idx_transfers_user_date`;
DROP INDEX IF EXISTS `idx_transactions_user_date`;
CREATE INDEX `idx_transactions_user_date` ON `transactions`(`user_id`, `date`) WHERE `deleted_at` IS NULL;
