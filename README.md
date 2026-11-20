**Try axum updates**

```bash
cargo install --locked bacon
bacon run 

curl -I http://127.0.0.1:8080/health
curl -siL http://127.0.0.1:8080/health
curl -si -X POST http://127.0.0.1:8080/data

```
