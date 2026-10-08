---
name: SLB's Soundboard
description: La barre d'action d'un jeu, posée sur de l'ardoise ; l'ambre ne s'allume que pour ce qui joue.
colors:
  ground: "oklch(0.215 0.024 220)"
  ground-deep: "oklch(0.18 0.02 220)"
  rail: "oklch(0.188 0.022 220)"
  panel: "oklch(0.245 0.026 218)"
  slot: "oklch(0.275 0.028 216)"
  slot-hover: "oklch(0.31 0.03 216)"
  line: "oklch(0.335 0.026 216)"
  line-strong: "oklch(0.43 0.03 214)"
  text: "oklch(0.95 0.013 85)"
  text-2: "oklch(0.83 0.018 200)"
  text-3: "oklch(0.72 0.022 205)"
  amber: "oklch(0.8 0.15 68)"
  amber-veil: "oklch(0.8 0.15 68 / 0.42)"
  sage: "oklch(0.79 0.11 158)"
  warn: "oklch(0.85 0.13 95)"
  brick: "oklch(0.52 0.16 32)"
  brick-hover: "oklch(0.57 0.17 32)"
  brick-text: "oklch(0.8 0.11 32)"
  selection: "oklch(0.47 0.06 212)"
typography:
  headline:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "24px"
    fontWeight: 650
    letterSpacing: "-0.015em"
  title:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "15.5px"
    fontWeight: 650
  body:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  slot-title:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
  label:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 600
  rail-label:
    fontFamily: "Segoe UI Variable Text, Segoe UI Variable, Segoe UI, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 600
  figure:
    fontFamily: "Cascadia Mono, Cascadia Code, Consolas, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
    fontFeature: "tnum"
  keycap:
    fontFamily: "Cascadia Mono, Cascadia Code, Consolas, ui-monospace, monospace"
    fontSize: "10.5px"
    fontWeight: 400
    lineHeight: 1.2
rounded:
  keycap: "5px"
  control-sm: "7px"
  control: "8px"
  menu: "9px"
  slot: "10px"
  chip: "13px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "14px"
  lg: "18px"
  gutter: "20px"
  page: "32px"
components:
  button-primary:
    backgroundColor: "{colors.text}"
    textColor: "{colors.ground-deep}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "34px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "34px"
  button-ghost-hover:
    backgroundColor: "{colors.slot}"
  button-ghost-danger:
    backgroundColor: "transparent"
    textColor: "{colors.brick-text}"
    rounded: "{rounded.control}"
    height: "34px"
  stop-all:
    backgroundColor: "transparent"
    textColor: "{colors.brick-text}"
    rounded: "{rounded.control-sm}"
    padding: "0 11px 0 8px"
    height: "28px"
  stop-all-armed:
    backgroundColor: "{colors.brick}"
    textColor: "#fff"
  stop-all-armed-hover:
    backgroundColor: "{colors.brick-hover}"
  slot:
    backgroundColor: "{colors.slot}"
    rounded: "{rounded.slot}"
    width: "minmax(132px, 1fr)"
  slot-key:
    backgroundColor: "{colors.ground-deep}"
    textColor: "{colors.text}"
    typography: "{typography.keycap}"
    rounded: "{rounded.keycap}"
    padding: "3px 5px 2px"
  input:
    backgroundColor: "{colors.ground-deep}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "0 10px"
    height: "34px"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.text-3}"
    rounded: "{rounded.control}"
    padding: "0 11px"
    height: "32px"
  tab-selected:
    backgroundColor: "{colors.slot}"
    textColor: "{colors.text}"
  rail-item:
    backgroundColor: "transparent"
    textColor: "{colors.text-3}"
    typography: "{typography.rail-label}"
    rounded: "{rounded.menu}"
    width: "64px"
  rail-item-current:
    backgroundColor: "{colors.slot}"
    textColor: "{colors.text}"
  state-chip:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    rounded: "{rounded.chip}"
    padding: "0 10px"
    height: "26px"
---

# Design System: SLB's Soundboard

## Overview

**Creative North Star: "La barre d'action"**

La bibliothèque se lit comme l'inventaire d'un jeu vidéo : chaque son est une case carrée d'échelle fixe, son raccourci est gravé en haut à gauche comme une touche de sort, et le son en cours se lit comme un temps de recharge. Tout le reste est de l'ardoise bleu-vert : un sol profond, des cases un ton plus clair au filet fin, un texte blanc os. La couleur vive n'existe que pour dire un état : ambre pour ce qui joue, sauge pour ce qui est en ligne, brique pour ce qui arrête ou supprime.

L'interface est dense et calme. Elle vit souvent dans une petite fenêtre à côté d'un jeu (1180×760 par défaut, 920×620 au minimum) et ne doit rien coûter au repos : aucune animation permanente, et l'interrogation du moteur audio ralentit au repos puis s'arrête quand la fenêtre est masquée dans la zone de notification. Le mouvement est réservé aux réponses à une action et aux entrées de panneaux.

Le système refuse la grille de cartes sombres à halo violet, le look gamer RGB et le panneau de configuration gris. Pas de dégradé décoratif, pas de lueur, pas de surtitre.

**Key Characteristics:**
- Sol d'ardoise bleu-vert, cases un ton plus clair, filets de 1px plutôt qu'ombres.
- Trois couleurs d'état, chacune à un seul rôle : ambre (lecture), sauge (en ligne), brique (arrêt et suppression).
- Interface en Segoe UI Variable ; touches, durées, compteurs et pourcentages en Cascadia Mono à chiffres tabulaires.
- Touches gravées : filet inférieur épaissi (2 à 3px) sur les touches, la marque SLB et le champ de raccourci.
- Signature : balayage conique « temps de recharge » proportionnel au temps restant, figé en voile uni en mouvement réduit.
- Icônes SVG maison au trait unique (grille 24px, trait 1.6px arrondi), jamais une police d'icônes ni un glyphe.

## Colors

Une ardoise froide et presque monochrome, ponctuée de trois couleurs d'état qui ne servent jamais à décorer.

### Primary
- **Ambre de recharge** (amber) : la seule couleur de « ce qui joue ». Anneau de 2px autour de la case en lecture, trait de tête du balayage conique, durée de la case en lecture, compteur « N son en cours » dans le bandeau d'état, titre de la ligne active en Communauté.
- **Voile d'ambre** (amber-veil) : le remplissage du balayage, et le voile uni qui le remplace en mouvement réduit.

### Secondary
- **Brique** (brick, brick-hover, brick-text) : arrêter, couper, supprimer, et les erreurs. « Tout arrêter » est discret au repos (texte brique sur fond transparent) et ne se remplit de brique qu'en présence de sons en cours. Le bouton stop d'une case en lecture est brique plein. « Supprimer » et les actions destructrices du menu sont des boutons fantômes au texte brique. Le bouton « Couper » enfoncé et la valeur d'un fader coupé passent en brique.

### Tertiary
- **Sauge** (sage) : l'état « en ligne ». Pastilles d'état, nœuds et segments du schéma de route audio, vu-mètre du micro dans le bandeau, interrupteur activé.
- **Jaune d'alerte** (warn) : état dégradé ou transitoire (démarrage, reprise, avertissement) sur les mêmes pastilles et nœuds que la sauge.

### Neutral
- **Ardoise profonde** (ground) : le sol de toute la fenêtre.
- **Ardoise de fond de puits** (ground-deep) : champs de saisie, journal, texte sur bouton clair, fond des touches gravées.
- **Ardoise du rail** (rail) : rail de navigation et bandeau d'état, un cran sous le sol.
- **Ardoise de panneau** (panel) : inspecteur, menus, messages, bannière de mise à jour.
- **Case** (slot, slot-hover) : fond des cases, onglet et entrée de rail actifs, survol des boutons fantômes.
- **Filet** (line) et **filet appuyé** (line-strong) : séparateurs et bords de case ; bords des contrôles, touches et champs.
- **Blanc os** (text), **texte secondaire** (text-2), **texte tertiaire** (text-3) : hiérarchie du texte ; le blanc os sert aussi de fond au bouton principal et à l'anneau de focus.
- **Sélection** (selection) : sélection de texte uniquement.

### Named Rules
**La règle de l'ambre qui joue.** L'ambre signifie « ce son joue maintenant », rien d'autre. Ni bouton principal, ni lien, ni sélection, ni décor ne le portent.

**La règle de la brique qui arrête.** La brique est réservée à l'arrêt, la coupure, la suppression et l'erreur. « Tout arrêter » ne se remplit qu'en présence de sons en cours ; au repos il reste du texte brique sans fond.

**La règle de la sauge en ligne.** Le vert sauge dit qu'un maillon de la chaîne audio fonctionne. Il ne sert pas de couleur de succès générique ni d'accent.

## Typography

**Display Font:** aucune ; l'interface n'a pas de titre d'affiche.
**Body Font:** Segoe UI Variable Text (avec Segoe UI Variable, Segoe UI, system-ui)
**Label/Mono Font:** Cascadia Mono (avec Cascadia Code, Consolas, ui-monospace)

**Character:** la sans de Windows, sobre et serrée, porte toute l'interface ; la mono de Windows porte tout ce qui se compte ou se tape au clavier, comme les gravures d'une touche.

### Hierarchy
- **Headline** (650, 24px, interlettrage -0.015em) : titre de page (Audio, Réglages, Communauté). L'état vide de la bibliothèque utilise un cran intermédiaire (650, 19px).
- **Title** (650, 15.5px) : titres de section dans les pages ; 15px pour l'en-tête de l'inspecteur.
- **Body** (400, 14px, 1.45) : texte courant ; paragraphes limités à 64-68ch.
- **Slot title** (600, 13px) : nom du son sous la case, tronqué sur une ligne.
- **Label** (600, 12.5px) : libellés de champs, boutons bascule, bandeau d'état ; indications et erreurs en 12px regular.
- **Rail label** (600, 11.5px) : libellés courts sous les icônes du rail.
- **Figure** (mono, 12px, chiffres tabulaires) : durées (11.5px), pourcentages, compteurs, lectures du moteur (13px).
- **Keycap** (mono, 10.5px, 1.2) : raccourcis gravés sur les cases et la touche Entrée de la case ciblée.

### Named Rules
**La règle de la gravure.** Tout ce qui se tape ou se compte (raccourci, durée, pourcentage, compteur, position « 2 / 12 ») est en Cascadia Mono à chiffres tabulaires, pour que les colonnes de durées s'alignent.

**La règle des polices système.** Seules les polices livrées avec Windows sont permises ; aucune police web n'est chargée.

## Layout

La fenêtre est une grille fixe : rail gauche de 76px (entrées de 64px de large, icône au-dessus d'un libellé court), espace de travail, et bandeau d'état bas de 36px sur toute la largeur. Le bandeau lit la chaîne audio de gauche à droite (Micro → Mixage → Câble, avec vu-mètre) et porte à droite le compteur de sons en cours et « Tout arrêter ».

La bibliothèque empile une barre d'outils (onglets de soundboards à gauche ; recherche et « Ajouter des sons » à droite), les messages, puis la grille de cases en remplissage automatique (minimum 132px par colonne, écart 18px vertical et 14px horizontal, marge intérieure 18px 20px). Sélectionner une case ouvre un inspecteur de 320px à droite (296px sous 1040px) sans réarranger le reste ; ses actions restent collées en bas.

Les pages de réglages sont une colonne unique de 940px au plus, marge 28px 32px, sections séparées par un filet supérieur et 24px d'écart. Rythme d'espacement : 4, 8, 14, 18, 20, 24, 32px.

Sous 760px, le rail devient une barre horizontale, le bandeau masque mixage, vu-mètre et compteur, et l'inspecteur devient un tiroir superposé de 340px au plus.

## Elevation & Depth

Le système est plat et tonal. La profondeur vient de l'échelle d'ardoise (rail et fond de puits sous le sol, panneau puis case au-dessus) et des filets de 1px. Les ombres n'existent que pour ce qui flotte au-dessus du contenu.

### Shadow Vocabulary
- **Menu flottant** (`box-shadow: 0 12px 30px oklch(0.1 0.02 220 / 0.55)`) : menu de soundboard.
- **Bannière** (`box-shadow: 0 14px 34px oklch(0.1 0.02 220 / 0.55)`) : bannière de mise à jour.
- **Tiroir** (`box-shadow: -12px 0 30px oklch(0.1 0.02 220 / 0.5)`) : inspecteur superposé en fenêtre étroite uniquement.

### Named Rules
**La règle du plat par défaut.** Cases, panneaux et contrôles n'ont aucune ombre. Seuls un menu, une bannière ou un tiroir qui recouvrent le contenu en portent une, diffuse et teintée d'ardoise. Jamais de lueur.

## Shapes

Des rectangles aux coins doucement arrondis, hiérarchisés par la taille : touches gravées (5px), petits contrôles du bandeau et outils de case (7px), boutons et champs (8px), menus et entrées de rail (9px), cases et bannière (10px). Les pastilles d'état et l'interrupteur sont entièrement arrondis. La case est toujours carrée (rapport 1:1), son illustration rognée au carré.

La forme signature est la touche gravée : bord de 1px et filet inférieur épaissi à 2px (touches de raccourci, indice « / » de la recherche, touche Entrée) ou 3px (marque SLB, champ de capture de raccourci). C'est la matière même du monde « barre d'action ».

## Components

### Buttons
Directs et sans ornement ; ils s'enfoncent légèrement au clic (échelle 0.97, 140ms).
- **Shape:** coins doucement arrondis (8px), hauteur 34px, icône 18px à gauche.
- **Primary:** fond blanc os, texte ardoise profonde, 600. Une seule action principale par vue (« Ajouter des sons »). Survol : blanc os légèrement assombri.
- **Ghost:** bord filet appuyé, fond transparent ; survol fond case.
- **Danger ghost:** texte brique ; survol bord brique et fond brique translucide.
- **Tout arrêter:** 28px, texte brique sans fond au repos, bord brique au survol ; rempli de brique avec texte blanc tant qu'au moins un son joue.
- **Icon button:** 32px carré, sans bord, texte secondaire ; survol fond case.
- **Hover / Focus:** survols seulement sur pointeur fin ; focus visible par contour blanc os de 2px décalé de 2px sur tout élément.

### Chips
- **Style:** pastille d'état de 26px, bord filet, texte secondaire, précédée d'une pastille de 8px (sauge en ligne, jaune transitoire, contour tertiaire à l'arrêt).

### Cards / Containers
- **Corner Style:** 10px pour les cases, 8px pour messages et encarts.
- **Background:** case sur sol ; panneau pour inspecteur, menus, messages.
- **Shadow Strategy:** aucune (voir Elevation & Depth).
- **Border:** filet 1px ; erreur en brique translucide.
- **Internal Padding:** 9-13px pour messages et encarts ; 14-18px pour l'inspecteur.

### Inputs / Fields
- **Style:** 34px, fond ardoise de fond de puits, bord filet appuyé, coins 8px ; libellé 12.5px 600 au-dessus, indication 12px tertiaire dessous.
- **Focus:** contour blanc os global ; le champ de capture de raccourci passe à un bord blanc os pendant l'écoute.
- **Error / Disabled:** message d'erreur 12px en texte brique ; désactivé à 45% d'opacité.
- **Curseurs :** piste 4px filet appuyé, pouce blanc os cerclé de sol ; la valeur s'affiche en mono à droite du libellé.
- **Interrupteur :** 38×22px ; activé = bord sauge et fond sauge translucide, pouce blanc os glissé de 16px.

### Navigation
- **Rail :** entrées 64px de large, icône 20px au-dessus d'un libellé 11.5px ; au repos texte tertiaire, page courante en blanc os sur fond case. Réglages est poussé en bas.
- **Onglets de soundboard :** 32px, texte tertiaire 600 et compteur mono ; l'onglet sélectionné prend le fond case. Débordement : fondu de masque sur le bord droit.

### Case de son (signature)
Carré d'échelle fixe : illustration rognée, ou à défaut une forme d'onde sur une ardoise teintée de façon stable (teinte 195-235 dérivée de l'identifiant). Raccourci gravé en haut à gauche ; titre et durée mono sous la case. Les outils (réglages, stop) apparaissent au survol, au focus, à la sélection ou pendant la lecture. En lecture : bord et anneau intérieur ambre de 2px, durée en ambre, et balayage conique de voile d'ambre dont le front avance avec le temps restant (transition linéaire de 220ms entre deux relevés). En mouvement réduit, le balayage devient un voile uni immobile. Sélectionnée : bord texte secondaire.

### Bandeau d'état
Chaîne audio lue comme un trajet : pastille d'état + Micro, chevron, Mixage, chevron, pastille + nom du câble, vu-mètre sauge de 44×4px. À droite, compteur ambre et « Tout arrêter ».

### Motion
Transitions courtes de 120-220ms ; courbe de sortie `cubic-bezier(0.23, 1, 0.32, 1)`. Les panneaux entrent depuis un état presque visible (inspecteur décalé de 10px, menu à 0.97, bannière décalée de 8px), jamais depuis rien. Aucune animation en boucle. Mouvement réduit : toutes les transitions sont ramenées à zéro.

## Do's and Don'ts

### Do:
- **Do** réserver l'ambre à ce qui joue : anneau, balayage, durée en cours, compteur de sons.
- **Do** garder « Tout arrêter » discret au repos et ne le remplir de brique que pendant la lecture.
- **Do** écrire touches, durées, pourcentages et compteurs en Cascadia Mono à chiffres tabulaires.
- **Do** dessiner toute nouvelle icône dans icons.tsx : grille 24px, trait 1.6px arrondi, `currentColor`, sans remplissage.
- **Do** utiliser le filet inférieur épaissi (2-3px) pour tout ce qui se lit comme une touche.
- **Do** suspendre tout relevé ou rafraîchissement quand la fenêtre est masquée, et figer le balayage en mouvement réduit.
- **Do** ouvrir les réglages d'un son dans le panneau latéral plutôt que dans la grille.

### Don't:
- **Don't** ajouter de surtitre ou d'étiquette au-dessus des titres.
- **Don't** utiliser de dégradé décoratif ni de lueur ; le seul dégradé est le balayage conique, qui mesure le temps.
- **Don't** lancer d'animation permanente ou en boucle, ni de rendu coûteux au repos.
- **Don't** utiliser une police d'icônes, des glyphes ou des emoji à la place des SVG maison.
- **Don't** charger une police web ; seules les polices système Windows sont permises.
- **Don't** utiliser la brique pour autre chose qu'arrêter, couper, supprimer ou signaler une erreur.
- **Don't** reproduire le look gamer RGB, le halo violet sur cartes sombres ou le panneau de configuration gris.
