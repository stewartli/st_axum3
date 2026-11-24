**Try axum updates**

```bash
cargo install --locked bacon
bacon run 

curl -siL http://127.0.0.1:8080/health
curl -si -X POST http://127.0.0.1:8080/data
curl -I http://127.0.0.1:8080/health
curl -H http://127.0.0.1:8080/health
curl -O -J http://127.0.0.1:8080/health
curl -N http://127.0.0.1:8080/health

```
