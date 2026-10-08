# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Application de bureau Windows (Tauri 2) dont l'interface est rendue en HTML/CSS dans une webview. Le langage visuel est celui du web, pas celui des contrôles natifs Windows.

## Users

Joueurs qui parlent sur Discord (ou dans le chat vocal d'un jeu) pendant qu'ils jouent. Ils déclenchent des sons par raccourci global sans quitter le jeu, et n'ouvrent la fenêtre que pour organiser leurs sons, régler un son ou ajuster le mixage. La fenêtre est souvent ouverte à côté du jeu : petite, sur un second écran, ou par-dessus un jeu en mode fenêtré.

## Product Purpose

Gérer des soundboards et injecter les sons dans le microphone de l'utilisateur, pour que ses interlocuteurs les entendent mêlés à sa voix. Réussir, c'est qu'un son parte à l'instant du raccourci, sans manipulation préalable et sans dégrader la partie en cours.

## Positioning

Une alternative gratuite, légère et sans télémétrie aux soundboards commerciaux (Voicemod, Soundpad). « Léger » signifie ici une consommation de ressources minimale : l'application ne doit jamais coûter d'images par seconde au jeu, même lorsqu'elle tourne en arrière-plan.

## Operating Context

- Les sons partent par raccourcis clavier globaux pendant que le jeu a le focus ; la fenêtre de l'application n'est pas visible à ce moment-là.
- Fermer la fenêtre laisse l'application active dans la zone de notification ; l'icône rouvre la fenêtre ou propose « Quitter ». Lancement au démarrage de Windows en option.
- Chemin audio : le micro physique et les sons sont mélangés puis envoyés vers un câble virtuel (VB-CABLE). L'utilisateur choisit `CABLE Output` comme micro dans Discord ou dans son jeu. Le micro est actif dès l'ouverture, il n'y a rien à démarrer.
- Les sons sont aussi joués dans la sortie audio habituelle (écoute), avec un volume réglable et la possibilité de la couper.
- Mises à jour signées, publiées sur les releases GitHub et proposées au démarrage.

## Capabilities and Constraints

- **Bibliothèque :** plusieurs soundboards, import de fichiers sons, réglages par son (volume, hauteur en demi-tons, vitesse, comportement au second appui), arrêt d'un son en cours.
- **Mixage :** gain et coupure séparés pour le micro, les sons et le général, plus le volume d'écoute ; les niveaux sont enregistrés dans la base de la bibliothèque.
- **Navigation actuelle :** Mes soundboards, Audio, Réglages (et Communauté quand elle est activée).
- **Fenêtre :** 1180×760 par défaut, au minimum 920×620.
- **Communauté auto-hébergée :** prévue mais pas encore livrée. Le code client et l'API Fastify/PostgreSQL existent, avec connexion Google et sessions protégées par le Gestionnaire d'identifiants Windows. Elle reste masquée tant que le serveur n'est pas déployé (`VITE_SLB_COMMUNITY=true` pour l'activer). Le design doit pouvoir l'accueillir sans la montrer aujourd'hui.
- **Pilote micro virtuel en mode noyau :** il existe dans `native/driver/`, mais il est désactivé par défaut faute de signature Microsoft (`VITE_SLB_DRIVER=true`). VB-CABLE est le chemin pris en charge.
- **Contrainte de ressources :** rendu, animations et traitements d'arrière-plan doivent rester sobres ; c'est une exigence produit, pas une simple optimisation.
- **Terminologie :** « soundboard », « son », « microphone virtuel », « câble virtuel », « écoute », « mixage ».

## Brand Commitments

- Nom : **SLB's Soundboard** ; marque courte « SLB ».
- Interface en français.
- Aucune télémétrie ; les diagnostics restent locaux.
- Gratuit.

## Evidence on Hand

- Aucun témoignage, aucune statistique d'usage, aucun benchmark de performance publié. Il ne faut pas en inventer.
- VB-CABLE est un logiciel tiers gratuit (donationware) de VB-Audio ; il faut le citer comme tel et ne jamais le présenter comme faisant partie du produit.

## Product Principles

1. **Le jeu passe d'abord.** Rien dans l'application ne doit voler des ressources, le focus ou l'attention au jeu.
2. **Zéro préparation.** Ce qui peut marcher dès l'ouverture marche dès l'ouverture : micro actif, sons prêts, raccourcis disponibles.
3. **Lisible en petit et d'un coup d'œil.** La fenêtre vit à côté du jeu, et l'état du chemin audio doit se lire sans effort.
4. **Gratuit veut dire honnête.** Pas de télémétrie, pas d'incitation commerciale, pas de fonctionnalité affichée avant d'être réellement disponible.

## Accessibility & Inclusion

- Toute l'application doit être utilisable au clavier, avec un focus visible ; les contrôles doivent être correctement nommés pour les lecteurs d'écran (axe-core fait déjà partie des tests).
- Respecter `prefers-reduced-motion` (déjà en place dans `App.css`).
