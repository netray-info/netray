FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=secret,id=NODE_AUTH_TOKEN,env=NODE_AUTH_TOKEN npm ci
COPY frontend/ .
RUN npm run build

FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src/
COPY data/ data/
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name tlsight) /

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S tlsight && adduser -S tlsight -G tlsight
WORKDIR /tlsight
COPY tlsight.example.toml tlsight.toml
ENV TLSIGHT_SERVER__BIND=0.0.0.0:8081
COPY --from=builder /tlsight .
RUN chown -R tlsight:tlsight /tlsight
USER tlsight
EXPOSE 8081 9090
CMD ["./tlsight", "tlsight.toml"]
