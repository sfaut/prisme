-- Test databases of the screenshots, on MariaDB: `shop` and `crm`.

CREATE DATABASE shop CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
USE shop;

CREATE TABLE customer (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  email VARCHAR(254) NOT NULL,
  name VARCHAR(100) NOT NULL,
  country CHAR(2) NULL,
  website VARCHAR(200) NULL,
  attributes JSON NULL,
  created_at DATETIME(6) NOT NULL,
  UNIQUE KEY email (email)
);

CREATE TABLE product (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  sku CHAR(8) NOT NULL,
  name VARCHAR(120) NOT NULL,
  price DECIMAL(10,2) NOT NULL,
  stock INT NOT NULL DEFAULT 0,
  specs JSON NULL,
  UNIQUE KEY sku (sku)
);

CREATE TABLE `order` (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  customer_id INT UNSIGNED NOT NULL,
  status ENUM('cart', 'paid', 'shipped', 'cancelled') NOT NULL DEFAULT 'cart',
  ordered_at DATETIME NOT NULL,
  total DECIMAL(10,2) NOT NULL DEFAULT 0,
  KEY ordered_at (ordered_at),
  CONSTRAINT order_customer FOREIGN KEY (customer_id) REFERENCES customer (id)
);

CREATE TABLE order_item (
  order_id INT UNSIGNED NOT NULL,
  product_id INT UNSIGNED NOT NULL,
  quantity SMALLINT UNSIGNED NOT NULL,
  unit_price DECIMAL(10,2) NOT NULL,
  PRIMARY KEY (order_id, product_id),
  CONSTRAINT item_order FOREIGN KEY (order_id) REFERENCES `order` (id),
  CONSTRAINT item_product FOREIGN KEY (product_id) REFERENCES product (id)
);

INSERT INTO customer (id, email, name, country, website, attributes, created_at)
SELECT
  seq,
  CONCAT(
    ELT(1 + seq % 12, 'ada', 'leon', 'ines', 'noah', 'chloe', 'hugo', 'lina', 'jules', 'emma', 'louis', 'zoe', 'malo'),
    '.',
    ELT(1 + (seq DIV 12) % 10, 'martin', 'dubois', 'garcia', 'weber', 'rossi', 'moreau', 'laurent', 'simon', 'michel', 'lefevre'),
    seq,
    '@example.org'
  ),
  CONCAT(
    ELT(1 + seq % 12, 'Ada', 'Léon', 'Inès', 'Noah', 'Chloé', 'Hugo', 'Lina', 'Jules', 'Emma', 'Louis', 'Zoé', 'Malo'),
    ' ',
    ELT(1 + (seq DIV 12) % 10, 'Martin', 'Dubois', 'Garcia', 'Weber', 'Rossi', 'Moreau', 'Laurent', 'Simon', 'Michel', 'Lefèvre')
  ),
  ELT(1 + seq % 7, 'FR', 'BE', 'ES', 'DE', 'IT', NULL, 'CH'),
  IF(seq % 3 = 0, CONCAT('https://', ELT(1 + seq % 12, 'ada', 'leon', 'ines', 'noah', 'chloe', 'hugo', 'lina', 'jules', 'emma', 'louis', 'zoe', 'malo'), seq, '.example.org'), NULL),
  CASE seq % 4
    WHEN 0 THEN NULL
    ELSE JSON_OBJECT(
      'plan', IF(seq % 2 = 1, 'pro', 'free'),
      'tags', CASE seq % 3 WHEN 0 THEN JSON_ARRAY() WHEN 1 THEN JSON_ARRAY('eu') ELSE JSON_ARRAY('beta', 'eu') END,
      'newsletter', seq % 5 <> 0
    )
  END,
  TIMESTAMP('2026-10-25 06:00:00') - INTERVAL ((1038 - seq) * 6) HOUR - INTERVAL ((seq * 37) % 3600) SECOND
FROM seq_1_to_1038;

INSERT INTO customer (id, email, name, country, website, attributes, created_at) VALUES
  (1039, 'noah.weber@example.org', 'Noah Weber', NULL, NULL, '{"plan":"pro","tags":["eu"]}', '2026-10-25 08:30:59'),
  (1040, 'ines.garcia@example.org', 'Inès Garcia', 'ES', 'https://ines.example.org', NULL, '2026-10-25 21:47:03'),
  (1041, 'leon.dubois@example.org', 'Léon Dubois', 'BE', NULL, '{"plan":"free","tags":[]}', '2026-10-26 09:15:40'),
  (1042, 'ada.martin@example.org', 'Ada Martin', 'FR', 'https://ada.example.org', '{"plan":"pro","tags":["beta","eu"]}', '2026-10-27 18:04:12');

INSERT INTO product (id, sku, name, price, stock, specs)
SELECT
  seq,
  CONCAT('SKU', LPAD(seq, 5, '0')),
  CONCAT(ELT(1 + seq % 8, 'Lampe', 'Chaise', 'Bureau', 'Étagère', 'Tapis', 'Coussin', 'Miroir', 'Horloge'), ' ', ELT(1 + seq % 5, 'Atlas', 'Brume', 'Cèdre', 'Dune', 'Écho')),
  ROUND(9.9 + (seq * 13.37) % 290, 2),
  (seq * 17) % 120,
  IF(seq % 4 = 0, NULL, JSON_OBJECT('color', ELT(1 + seq % 4, 'blue', 'oak', 'white', 'black'), 'weight_kg', ROUND(0.5 + (seq * 0.7) % 25, 1)))
FROM seq_1_to_40;

INSERT INTO `order` (id, customer_id, status, ordered_at, total)
SELECT
  seq,
  1 + (seq * 7) % 1042,
  ELT(1 + seq % 4, 'cart', 'paid', 'shipped', 'cancelled'),
  TIMESTAMP('2026-10-27 12:00:00') - INTERVAL (seq * 2) HOUR,
  0
FROM seq_1_to_3000;

INSERT INTO order_item (order_id, product_id, quantity, unit_price)
SELECT o.seq, 1 + (o.seq * 3 + k.seq * 7) % 40, 1 + (o.seq + k.seq) % 3, p.price
FROM seq_1_to_3000 AS o
JOIN seq_1_to_3 AS k ON k.seq <= 1 + o.seq % 3
JOIN product AS p ON p.id = 1 + (o.seq * 3 + k.seq * 7) % 40;

UPDATE `order` AS o
JOIN (SELECT order_id, SUM(quantity * unit_price) AS total FROM order_item GROUP BY order_id) AS i ON i.order_id = o.id
SET o.total = i.total;

CREATE VIEW customer_total AS
SELECT c.id, c.name, COUNT(o.id) AS orders, COALESCE(SUM(o.total), 0) AS total
FROM customer AS c
LEFT JOIN `order` AS o ON o.customer_id = c.id AND o.status <> 'cancelled'
GROUP BY c.id, c.name;

CREATE FUNCTION vat(amount DECIMAL(10,2)) RETURNS DECIMAL(10,2) DETERMINISTIC
RETURN ROUND(amount * 0.2, 2);

DELIMITER //
CREATE PROCEDURE refresh_stats()
BEGIN
  ANALYZE TABLE customer, product, `order`, order_item;
END //
DELIMITER ;

CREATE EVENT purge_carts ON SCHEDULE EVERY 1 DAY DISABLE
DO DELETE FROM `order` WHERE status = 'cart' AND ordered_at < NOW() - INTERVAL 30 DAY;

CREATE TRIGGER order_item_total AFTER INSERT ON order_item FOR EACH ROW
UPDATE `order` SET total = total + NEW.quantity * NEW.unit_price WHERE id = NEW.order_id;

CREATE DATABASE crm CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
USE crm;

CREATE TABLE company (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  name VARCHAR(120) NOT NULL,
  siren CHAR(9) NOT NULL,
  UNIQUE KEY siren (siren)
);

CREATE TABLE contact (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  company_id INT UNSIGNED NOT NULL,
  email VARCHAR(254) NOT NULL,
  phone VARCHAR(20) NULL,
  CONSTRAINT contact_company FOREIGN KEY (company_id) REFERENCES company (id)
);

CREATE TABLE note (
  id INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  contact_id INT UNSIGNED NOT NULL,
  body TEXT NOT NULL,
  created_at DATETIME NOT NULL,
  CONSTRAINT note_contact FOREIGN KEY (contact_id) REFERENCES contact (id)
);

INSERT INTO company (id, name, siren) VALUES
  (1, 'Atelier Brume', '512345678'),
  (2, 'Cèdre & Fils', '498765432'),
  (3, 'Dune Logistique', '823456719');

INSERT INTO contact (id, company_id, email, phone) VALUES
  (1, 1, 'claire@brume.example.org', '+33 1 23 45 67 89'),
  (2, 2, 'marc@cedre.example.org', NULL),
  (3, 3, 'sofia@dune.example.org', '+33 4 56 78 90 12');

INSERT INTO note (contact_id, body, created_at) VALUES
  (1, 'Rappeler pour le devis du T4.', '2026-10-20 10:12:00'),
  (3, 'Livraison prévue semaine 45.', '2026-10-24 16:40:00');
