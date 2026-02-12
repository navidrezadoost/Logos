# Logos

<p align="center">
  <img src="https://img.shields.io/badge/Status-Pre--Alpha-red.svg" alt="Status">
  <img src="https://img.shields.io/badge/Built_With-Rust-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Renderer-WGPU-blue.svg" alt="WGPU">
</p>

**Logos** is a high-performance, open-source design tool built to compete with industry standards like Figma. It leverages the power of **Rust** and **WebGPU** to deliver native performance on the desktop and the web.

> 📘 **Read the Vision**: [Executive Technical Roadmap](./logos-plugin-system/docs/STRATEGY_ROADMAP.md)

## 🚀 Mission

To create a professional design tool that is:
- **Fast**: Native performance using WGPU and Rust.
- **Collaborative**: Real-time multiplayer built-in using CRDTs (`yrs`).
- **Open**: Fully open-source and extensible.
- **Private**: Offline-first and local file storage.

## 🛠 Tech Stack

- **Language**: Rust (2021/2024 Edition)
- **Graphics**: `wgpu`, `piet-gpu` (Vector optimization)
- **Application Shell**: Tauri
- **State Management**: `yrs` (CRDTs)
- **Layout**: `Taffy` (Flexbox/Grid)

## 📅 Roadmap

### Week 1: Core & Architecture (Completed)
- [x] Defined Object Model (`Node`, `NodeType`)
- [x] Established Monorepo Structure
- [x] Repository Rebranding

### Week 2: Rendering Pipeline (In Progress)
- [x] WGPU Integration (`logos-render`)
- [ ] Primitive Component Rendering
- [ ] Shader Implementation

### Week 3+: Editor & Interaction
- [ ] Selection Logic
- [ ] Property Panel
- [ ] Tool Implementations

## 📦 Getting Started

### Prerequisites
- Rust (latest stable)
- Node.js (for Tauri frontend)

### Building
```bash
cargo build -p logos-core
```
