# SLB's Soundboard

Gestionnaire de soundboards Windows avec microphone virtuel, bibliothèque locale, raccourcis globaux et partage communautaire auto-hébergé.

## Stack

- Tauri 2, React, TypeScript et Vite
- Rust et WASAPI pour le moteur audio
- C++/WDK pour le microphone virtuel Windows
- Fastify, PostgreSQL et stockage fichiers local pour la communauté

## Démarrer

```powershell
npm install
npm run dev:desktop
```

API locale :

```powershell
npm run dev:api
```

