# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p vak-server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /app/target/release/vak-server /vak
ENV VAK_HOSTED=1
EXPOSE 8080
ENV PORT=8080
ENTRYPOINT ["/vak"]
