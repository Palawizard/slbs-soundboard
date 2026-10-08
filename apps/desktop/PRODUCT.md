# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Interface React rendue dans une fenêtre Tauri 2, distribuée uniquement pour Windows. Le langage visuel est celui du web, pas celui d'un OS natif.

## Users

Des joueurs qui discutent sur Discord ou dans le chat vocal d'un jeu. En pleine partie, ils déclenchent des sons pour leurs amis avec des raccourcis globaux, sans quitter le jeu. Ils ouvrent la fenêtre surtout pour préparer : importer des sons, organiser leurs soundboards, assigner des raccourcis, régler le micro. Pendant la partie, l'app reste dans la zone de notification.

## Product Purpose

Mélanger le microphone de l'utilisateur et ses sons, puis envoyer le résultat vers un câble audio virtuel (VB-CABLE). Ce câble devient le micro dans Discord et dans les jeux. Le produit réussit quand un son part au bon moment, au bon volume, par un simple raccourci, et que le micro fonctionne dès l'ouverture de l'app sans rien démarrer.

## Positioning

Une alternative gratuite à Voicemod, Soundpad et consorts, qui fonctionne entièrement en local. La bibliothèque reste sur la machine, il n'y a pas de compte obligatoire et aucune télémétrie : les diagnostics restent locaux.

## Operating Context

- Usage principal en arrière-plan : la fenêtre fermée, l'app reste dans la zone de notification et les raccourcis globaux restent actifs.
- Prérequis externe : VB-CABLE, installé à part. Dans Discord ou dans le jeu, l'utilisateur choisit `CABLE Output` comme micro.
- Sections actuelles : Mes soundboards (bibliothèque), Audio (micro, câble virtuel, mixage, écoute de contrôle) et Réglages (lancement au démarrage de Windows, etc.). Les mises à jour se proposent au démarrage.
- Communauté (partage auto-hébergé, connexion Google) : développée, mais masquée derrière `VITE_SLB_COMMUNITY` tant que le serveur n'est pas déployé.
- Pilote de capture noyau : présent, mais désactivé par défaut (`VITE_SLB_DRIVER`) faute de signature Microsoft.

## Capabilities and Constraints

- Stack : Tauri 2, React 19, TypeScript, Vite, Zustand et TanStack Query. Moteur audio en Rust/WASAPI. API communautaire en Fastify et PostgreSQL.
- Interface en français.
- Usage léger en arrière-plan : l'interface ne doit pas consommer de ressources pendant les parties (pas d'animation permanente, pas de rendu coûteux au repos).
- Terminologie : « soundboard », « son », « microphone virtuel », « câble virtuel », « écoute ».
- À décider : la licence. Le projet se veut open source, mais le dépôt ne contient aucun fichier LICENSE. Aucune communication ne doit affirmer « open source » avant que la licence soit choisie.

## Brand Commitments

Nom : « SLB's Soundboard ». La marque actuelle se limite au monogramme texte « SLB » dans la barre latérale. Aucun logo ni ressource graphique dédiée n'existe : `public/` ne contient que les SVG par défaut de Tauri et Vite.

## Evidence on Hand

Aucun témoignage, chiffre d'usage, capture marketing ni presse. Il ne faut pas en inventer.

## Product Principles

1. Le son part tout de suite : la latence et la fiabilité du déclenchement passent avant tout le reste.
2. Rien à démarrer : le micro virtuel est actif dès l'ouverture, et la configuration se fait une fois puis disparaît.
3. Discret en arrière-plan : l'app ne doit jamais coûter de performances au jeu.
4. Local par défaut : les sons et les données restent sur la machine de l'utilisateur.

## Accessibility & Inclusion

Exigence explicite : l'interface doit être entièrement utilisable au clavier et compatible avec les lecteurs d'écran (rôles, `aria-label`, focus visible). Les tests incluent déjà axe-core et doivent rester verts.
