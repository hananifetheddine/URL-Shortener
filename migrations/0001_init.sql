-- UUID générés par l'application, stockés en CHAR(36).
-- DATETIME(6) en UTC naïf : pas de conversion de fuseau ni de limite 2038,
-- contrairement à TIMESTAMP.

CREATE TABLE users (
    id CHAR(36) NOT NULL,
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    created_at DATETIME(6) NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY users_email_unique (email)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE links (
    id CHAR(36) NOT NULL,
    user_id CHAR(36) NOT NULL,
    code VARCHAR(32) NOT NULL,
    original_url VARCHAR(2048) NOT NULL,
    expires_at DATETIME(6) NULL,
    created_at DATETIME(6) NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY links_code_unique (code),
    KEY links_user_created_idx (user_id, created_at),
    CONSTRAINT links_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE clicks (
    id CHAR(36) NOT NULL,
    link_id CHAR(36) NOT NULL,
    clicked_at DATETIME(6) NOT NULL,
    country VARCHAR(8) NULL,
    referrer VARCHAR(1024) NULL,
    PRIMARY KEY (id),
    KEY clicks_link_clicked_idx (link_id, clicked_at),
    CONSTRAINT clicks_link_fk FOREIGN KEY (link_id) REFERENCES links (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
