---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: ["src/SettingsPage.tsx","src/CommunityPage.tsx","src/updates.tsx"]
---

# Surface: application de bureau (toutes les pages)

Scope: refonte complète de l'interface de apps/desktop (bibliothèque, Audio, Réglages, Communauté masquée, bannière de mise à jour). Mode: Operate.

Audience et tâche: joueurs sur Discord ; tâche n°1 à l'ouverture = retrouver et jouer un son vite. La préparation (import, raccourcis, réglages par son) passe au second plan. Fenêtre souvent petite, à côté du jeu.
Contraintes: utilisable au clavier et au lecteur d'écran ; aucune animation permanente ni coût au repos ; polices système Windows uniquement ; toutes les fonctions actuelles conservées.
À éviter (utilisateur): look gamer RGB, clinique, chargé, enfantin/gadget.

## Direction contract

THESIS: La bibliothèque est l'inventaire d'un jeu : chaque son est une case dont le raccourci est gravé comme une touche de sort, et le son en cours se lit comme un temps de recharge. Refuse la grille de cartes sombres à halo violet et le panneau de configuration gris.

OWN-WORLD: Fond ardoise bleu-vert profond, cases un ton plus clair au bord fin, texte blanc os ; ambre chaud réservé à ce qui joue (anneau + balayage), rouge brique réservé à Tout arrêter et aux suppressions ; vert sauge pour l'état « en ligne ». Segoe UI Variable pour l'interface, Cascadia Mono pour touches, durées et pourcentages. Icônes SVG au trait unique 1.5px. Aucun dégradé décoratif, aucune lueur.

STORY: Le joueur ouvre la fenêtre, tape deux lettres, voit la case, clique ; la case se balaye en ambre pendant la lecture. Il sait d'un coup d'œil en bas que micro et câble sont en ligne. Régler un son ouvre un panneau latéral, jamais un encombrement de la grille.

FIRST VIEWPORT: Rail gauche 64px (marque SLB, Sons, Audio, Réglages, icône + libellé court). Barre haute : onglets de soundboards à gauche, recherche au centre, Ajouter et Tout arrêter à droite. Grille dense de cases carrées identiques (vignette, touche gravée en haut à gauche, titre et durée sous la vignette) occupant tout le reste. Bandeau d'état bas 36px : Micro ● → Mixage → Câble ●, niveau micro. Panneau inspecteur 320px à droite à la sélection d'une case.

FORM: Barre d'action et inventaire de jeu vidéo, candidat 4 de la liste ordonnée (métro, juke-box, risographie, barre d'action, tableau à palettes, cassette, Panini) ; seed 59c66593. Rehaussements : un seul anneau (atlas d'étoiles), accent rare (marketing monochrome), échelle fixe des cases (folio botanique), durées alignées en colonne tabulaire (jaquette cassette), contraste net et balayage figé en mouvement réduit (Ikeda). Signature : balayage conique « temps de recharge » proportionnel au temps restant.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
