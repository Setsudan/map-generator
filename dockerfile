# Use the official Rust image (Linux-based)
FROM rust:latest

# Set the working directory inside the container
WORKDIR /app

# Copy the project files into the container
COPY . .

# Build the project in release mode (faster execution)
RUN cargo build --release

# The default command runs the compiled binary
CMD ["./target/release/fantasy_map"]