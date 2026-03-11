FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src/
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name tlsight) /

FROM alpine:3.21
RUN addgroup -S tlsight && adduser -S tlsight -G tlsight
WORKDIR /tlsight
COPY tlsight.prod.toml tlsight.toml
COPY --from=builder /tlsight .
RUN chown -R tlsight:tlsight /tlsight
USER tlsight
EXPOSE 8081 9090
CMD ["./tlsight", "tlsight.toml"]
