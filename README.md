# Raccourcisseur d'URL

API HTTP en Rust (Axum, MySQL, Redis) : comptes, liens courts, redirection et statistiques de clics.

## Prérequis

- Rust stable (1.94 ou plus récent, exigé par SQLx 0.9) : <https://rustup.rs>
- MySQL 8, installé nativement
- Redis 7, installé nativement

Le développement ne passe pas par Docker. Le `Dockerfile` sert uniquement à construire l'image de déploiement.

### MySQL

macOS :

```bash
brew install mysql
brew services start mysql
mysql_secure_installation
```

Linux (Debian/Ubuntu) :

```bash
sudo apt update
sudo apt install mysql-server
sudo systemctl enable --now mysql
sudo mysql_secure_installation
```

Créer la base (le schéma est appliqué au démarrage de l'API) :

```sql
CREATE DATABASE url_shortener CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
CREATE USER 'shortener'@'127.0.0.1' IDENTIFIED BY 'password';
GRANT ALL PRIVILEGES ON url_shortener.* TO 'shortener'@'127.0.0.1';
FLUSH PRIVILEGES;
```

### Redis

macOS :

```bash
brew install redis
brew services start redis
redis-cli ping
```

Linux (Debian/Ubuntu) :

```bash
sudo apt update
sudo apt install redis-server
sudo systemctl enable --now redis-server
redis-cli ping
```

La réponse attendue est `PONG`.

Sous Windows, les installeurs officiels ou `winget install Oracle.MySQL` et `winget install Redis.Redis` conviennent. Les commandes `cargo` ci-dessous sont identiques.

## Configuration

```bash
cp .env.example .env
```

Renseigner au minimum `DATABASE_URL`, `REDIS_URL` et `JWT_SECRET` (32 caractères minimum). Les autres variables ont une valeur par défaut, documentée dans `.env.example`.

`DATABASE_URL` ressemble à :

```text
mysql://shortener:password@127.0.0.1:3306/url_shortener
```

## sqlx-cli

L'API applique `migrations/` toute seule au démarrage. La CLI reste utile pour inspecter ou rejouer les migrations à la main.

```bash
cargo install sqlx-cli --version 0.9.0 --no-default-features --features mysql,rustls,mysql-rsa
```

Le feature `mysql-rsa` est nécessaire pour un MySQL local sans TLS qui authentifie avec `caching_sha2_password`.

```bash
sqlx database create
sqlx migrate run
sqlx migrate info
```

Les commandes lisent `DATABASE_URL` depuis l'environnement ou le fichier `.env`.

## Lancer l'API

```bash
cargo run
```

Écoute par défaut : `http://127.0.0.1:8080`.

## Exemples curl

```bash
curl -s -X POST http://127.0.0.1:8080/auth/register \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"password-123"}'

curl -s -X POST http://127.0.0.1:8080/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"ada@example.com","password":"password-123"}'
```

Reprendre le champ `token` :

```bash
TOKEN=coller-le-jwt

curl -s -X POST http://127.0.0.1:8080/links \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"url":"https://example.com/docs","custom_code":"docs"}'

curl -s http://127.0.0.1:8080/links \
  -H "authorization: Bearer $TOKEN"

curl -s -D - -o /dev/null http://127.0.0.1:8080/docs

curl -s http://127.0.0.1:8080/links/docs/stats \
  -H "authorization: Bearer $TOKEN"

curl -s -X DELETE -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:8080/links/docs \
  -H "authorization: Bearer $TOKEN"
```

`GET /{code}` répond `302` avec `Location` et `Cache-Control: no-store`. Un code inconnu ou expiré répond `404`. Les clics sont écrits en arrière-plan : les statistiques peuvent arriver quelques centaines de millisecondes après la redirection.

Le pays est lu depuis `CF-IPCountry` ou `X-Country-Code`. Le referrer est l'en-tête `Referer`.

## Tests

Les tests d'intégration ont besoin de MySQL, de Redis et des variables de `.env`.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Image de déploiement

```bash
docker build -t url-shortener .
docker run --rm -p 8080:8080 --env-file .env -e BIND_ADDR=0.0.0.0:8080 url-shortener
```

MySQL et Redis ne sont pas dans l'image : leurs URL doivent être joignables depuis le conteneur.
