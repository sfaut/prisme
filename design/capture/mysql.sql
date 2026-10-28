-- Test database of the screenshots, on MySQL: `billing`.

CREATE DATABASE billing CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;
USE billing;

CREATE TABLE invoice (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  number CHAR(10) NOT NULL,
  customer_ref INT UNSIGNED NOT NULL,
  issued_on DATE NOT NULL,
  amount DECIMAL(12,2) NOT NULL,
  status ENUM('draft', 'sent', 'paid', 'late') NOT NULL DEFAULT 'draft',
  items JSON NULL,
  UNIQUE KEY number (number)
);

CREATE TABLE payment (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  invoice_id INT UNSIGNED NOT NULL,
  paid_at DATETIME NOT NULL,
  amount DECIMAL(12,2) NOT NULL,
  method ENUM('card', 'transfer', 'cheque') NOT NULL,
  CONSTRAINT payment_invoice FOREIGN KEY (invoice_id) REFERENCES invoice (id)
);

SET SESSION cte_max_recursion_depth = 10000;

INSERT INTO invoice (id, number, customer_ref, issued_on, amount, status, items)
WITH RECURSIVE n (i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 800)
SELECT
  i,
  CONCAT('F2026', LPAD(i, 5, '0')),
  1 + (i * 11) % 1042,
  DATE('2026-10-27') - INTERVAL (800 - i) DIV 3 DAY,
  ROUND(20 + (i * 47.31) % 900, 2),
  ELT(1 + i % 4, 'draft', 'sent', 'paid', 'late'),
  JSON_ARRAY(JSON_OBJECT('sku', CONCAT('SKU', LPAD(1 + i % 40, 5, '0')), 'qty', 1 + i % 3))
FROM n;

INSERT INTO payment (invoice_id, paid_at, amount, method)
SELECT id, TIMESTAMP(issued_on) + INTERVAL 10 DAY, amount, ELT(1 + id % 3, 'card', 'transfer', 'cheque')
FROM invoice
WHERE status = 'paid';
