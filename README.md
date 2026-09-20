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

## Microphone virtuel

L'application mélange votre microphone et vos sons, puis envoie le résultat vers
un câble audio virtuel. La capture de ce câble devient votre microphone dans
Discord, dans vos jeux et dans tout logiciel de communication.

1. Installez [VB-CABLE](https://vb-audio.com/Cable/), un logiciel gratuit
   (donationware) publié par VB-Audio.
2. Dans l'onglet **Audio**, choisissez votre microphone, puis
   `CABLE Input (VB-Audio Virtual Cable)` comme câble virtuel, et démarrez.
3. Dans Discord ou votre jeu, sélectionnez
   `CABLE Output (VB-Audio Virtual Cable)` comme microphone.

Activez l'écoute locale pour vous entendre jouer les sons sans les renvoyer dans
le câble.

Un pilote de capture noyau est présent dans `native/driver/`. Il exige une
signature Microsoft pour être distribué, il n'est donc pas utilisé par défaut.
Compilez avec `VITE_SLB_DRIVER=true` pour retrouver ses contrôles.

## Mises à jour

Les versions publiées sont signées avec la clé de mise à jour du projet et
déposées sur les releases GitHub. L'application vérifie
`releases/latest/download/latest.json` au démarrage et propose l'installation.
Publier une version revient à pousser un tag :

```powershell
npm version --workspace @slb/desktop <version>   # puis alignez tauri.conf.json
git tag v<version>
git push origin v<version>
```

Le workflow `.github/workflows/release.yml` compile, signe et publie. Il attend
le secret `TAURI_SIGNING_PRIVATE_KEY` (et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
s'il y en a un).

## Communauté

Le service communautaire est développé mais n'est pas déployé. Il reste masqué
tant que le serveur et le client Google n'existent pas ; compilez avec
`VITE_SLB_COMMUNITY=true` pour l'activer.

Le client utilise `http://127.0.0.1:3000` en développement. Définissez `SLB_COMMUNITY_API_URL` sur une origine HTTPS au moment de la compilation pour une distribution. Les sessions communautaires sont protégées par le Gestionnaire d’identifiants Windows et les diagnostics restent locaux, sans télémétrie.

La procédure de packaging signé, le cycle de vie du pilote et les contrôles obligatoires sont décrits dans [RELEASE.md](RELEASE.md).
