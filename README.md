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

Le client utilise `http://127.0.0.1:3000` en développement. Définissez `SLB_COMMUNITY_API_URL` sur une origine HTTPS au moment de la compilation pour une distribution. Les sessions communautaires sont protégées par le Gestionnaire d’identifiants Windows et les diagnostics restent locaux, sans télémétrie.

La procédure de packaging signé, le cycle de vie du pilote et les contrôles obligatoires sont décrits dans [RELEASE.md](RELEASE.md).
