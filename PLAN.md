# NoteCraft — plan projet

> v0.1 — prévisionnel du 25 novembre 2026. Le CdC, les use cases, les lots et les US ci-dessous sont un premier jet. La salle A (spec) les affine pendant les phases 1 à 3 ; ce qui change ici doit d'abord changer dans spec/.

Suivi des statuts : artefact « NoteCraft — suivi projet » (https://claude.ai/artifact/XdEUkwnuAanBPifA7xtQe6).

## Feuille de route

### P0 — Lancement (Tristan)
Cadrer le projet et obtenir le feu vert de la communauté.

- [x] **P0-01** Choisir le nom — _Fait_ — NoteCraft, en écho au logiciel de référence. PageCraft reste l'alternative si les mainteneurs le demandent.
- [x] **P0-02** Proposer la fusion NoteCraft/BaseCraft/TaskCraft sur #2134 — _Fait_ — Commentaire publié le 23 octobre.
- [x] **P0-03** Obtenir un feu vert — _Fait_ — matbrgz a répondu « go ahead ». Pas encore de validation d'un mainteneur storytold.
- [x] **P0-04** Rédiger le prompt de la salle A (spec) — _Fait_ — notecraft-spec-prompt.md : clean-room, REA côté spec seulement, chaîne CdC → UC → lots → US.
- [x] **P0-05** Rédiger la méthodo des sous-agents — _Fait_ — notecraft-methodo-agents.md : lead tech, testeur, dev, reviewer, auditeur, intégrateur.
- [x] **P0-06** Créer les deux espaces de travail — _Fait_ — notecraft-spec/ (salle A, seul endroit où REA tourne) et le dépôt notecraft/ (salle B, format Craft). Jamais de lien de l'un vers l'autre sauf la copie de spec/.
- [ ] **P0-07** Signaler l'usage de REA aux mainteneurs — _À faire_ — Les règles Craft interdisent d'inspecter le logiciel de référence. À annoncer sur #2134 avant toute PR, avec la séparation des salles.
- [ ] **P0-08** Confirmer le nom auprès de storytold — _À faire_ — Deux projets indépendants s'appellent déjà NoteCraft (clones OneNote). Faire acter le nom sur #2134.

### P1 — Analyse (Agent spec (salle A))
Comprendre ce qu'on construit et pour qui, avant d'écrire une seule exigence.

- [ ] **P1-01** Lire les règles de la suite Craft — _À faire_ — craftrules (never-crash, progress-docs), AGENTS.md de WordCraft et GridCraft, issues #2131, #2142, #2143, #2174, #2183. Résumé dans plan/notes/rules.md.
- [ ] **P1-02** Écrire le brief produit — _À faire_ — Problème, personas (créatif solo, petit studio, agent IA), principes (local-first, Git, agents, modes), ce qu'on ne fait pas.
- [ ] **P1-03** Paysage concurrent — _À faire_ — AppFlowy, AFFiNE, Docmost, Anytype, Obsidian, Logseq : fonctionnalités, licences, modèle de stockage. Pages publiques uniquement, jamais leur code.
- [ ] **P1-04** Observer le logiciel de référence en boîte noire — _À faire_ — Scénarios sur un espace de test fictif, captures de sa fenêtre seulement, journal daté dans plan/reference/observations.md.
- [ ] **P1-05** Investigation REA (comportement seulement) — _À faire_ — Sorties brutes dans plan/rea/ (non versionné). Seules des descriptions de comportement taguées [rea] passent dans spec/.
- [ ] **P1-06** Glossaire — _À faire_ — Espace, page, bloc, base, propriété, vue, ligne, tâche, relation, rollup.

### P2 — Planification (Agent spec + Tristan)
Fixer le quoi : exigences, use cases, découpage en lots.

- [ ] **P2-01** Finaliser le cahier des charges — _À faire_ — Onglet « Cahier des charges » : exigences EF et ENF, contraintes, risques.
- [ ] **P2-02** Finaliser les use cases — _À faire_ — Onglet « Use cases » : acteur, flux principal, flux d'erreur, EF couvertes.
- [ ] **P2-03** Finaliser les lots — _À faire_ — Onglet « Lots et US » : objectif, critère de sortie (démo), dépendances.
- [ ] **P2-04** Porte 1 : validation humaine du périmètre — _À faire_ — Tristan valide le CdC, les UC et l'ordre des lots. Rien ne part en conception sans ce feu vert.

### P3 — Conception (Agent spec + lead tech)
Fixer le comment, au niveau comportement et architecture, puis découper en US prêtes à développer.

- [ ] **P3-01** Modèle de données — _À faire_ — Entités, identifiants stables (survivent aux renommages et aux fusions Git), invariants. Une ligne de base est une page ; une tâche est une ligne.
- [ ] **P3-02** Format sur disque — _À faire_ — Pages en Markdown + front matter dans le projet .craft ; bases en texte fusionnable avec cache SQLite reconstruit ; décision consignée dans un ADR.
- [ ] **P3-03** Catalogue de commandes et surface MCP — _À faire_ — Chaque action = un CommandSpec ; mêmes commandes depuis l'UI, les raccourcis, la CLI, le canal de contrôle et MCP.
- [ ] **P3-04** Architecture en crates (L0–L6) et ADR — _À faire_ — Proposition de départ : model, md, store, query, search, engine, mcp, cli, ui-egui. Le lead tech tranche.
- [ ] **P3-05** Rédiger les US détaillées — _À faire_ — Critères Given/When/Then, cas d'erreur, commandes, crates touchées, taille S/M (une L est redécoupée).
- [ ] **P3-06** Table de traçabilité — _À faire_ — Chaque EF couverte par au moins un UC, chaque UC par au moins une US, chaque US par au moins un test d'acceptation.
- [ ] **P3-07** Porte 2 : contrôle « prêt à développer » — _À faire_ — Traçabilité complète, archi validée par Tristan, US du lot 0 et du lot 1 au statut prêt. Inspiré du contrôle de BMAD.

### P4 — Implémentation (Sous-agents (salle B))
Livrer lot par lot. Le détail est dans l'onglet « Lots et US ».

- [ ] **P4-01** Poser .claude/agents/ et AGENTS.md dans le dépôt — _À faire_ — Les 6 sous-agents de la méthodo, AGENTS.md repris de WordCraft, CLAUDE.md en lien vers AGENTS.md.
- [ ] **P4-02** Lot 0 livré + rétro — _À faire_ — Démo du critère de sortie, rétro, leçons ajoutées à AGENTS.md.
- [ ] **P4-03** Lot 1 livré + rétro — _À faire_ — Démo, rétro, docs Craft à jour.
- [ ] **P4-04** Lot 2 livré + rétro — _À faire_ — Démo, rétro, docs Craft à jour.
- [ ] **P4-05** Lot 3 livré + rétro — _À faire_ — Démo, rétro. À partir de là, le suivi des US peut vivre dans NoteCraft lui-même.
- [ ] **P4-06** Lot 4 livré + rétro — _À faire_ — Démo, rétro.

### P5 — Publication (Tristan)
Faire entrer NoteCraft dans la suite Craft.

- [ ] **P5-01** Docs au format Craft — _À faire_ — ROADMAP.md, docs/roadmap.md (lots = jalons M0…), docs/gaps.md, docs/*-parity.md, ATTRIBUTION.md.
- [ ] **P5-02** Démo publique — _À faire_ — Vidéo courte ou build web (wasm) du scénario de fin du lot 3.
- [ ] **P5-03** Proposer le dépôt à storytold — _À faire_ — Transfert dans l'organisation ou PR, selon ce que les mainteneurs préfèrent.
- [ ] **P5-04** Annoncer sur #2134 et #2174 — _À faire_ — Lien vers le dépôt, état des lots, appel à contributions.

## Cahier des charges

Les apps Craft reconstruisent en Rust, en clean-room, les outils propriétaires du quotidien. Il n'existe pas d'espace pages + bases + tâches local-first dans la suite ; trois propositions séparées couvrent ce besoin. NoteCraft les réunit en une seule app à modes.

**Objectifs**
- Un espace de pages et de bases de données qui appartient à l'utilisateur : des fichiers lisibles sur son disque.
- Les agents IA comme utilisateurs de premier plan, via la CLI et MCP.
- Une intégration native aux projets .craft du reste de la suite.
- Un import fidèle depuis le logiciel de référence pour permettre la migration.

**Hors périmètre**
- Un service cloud ou des comptes utilisateurs.
- Un modèle d'IA intégré : les agents viennent de l'extérieur via MCP.
- La co-édition en temps réel avant le lot 5.
- La reproduction pixel par pixel de l'interface de référence.

**Personas**
- Créatif solo (`solo`) : Écrit briefs, scripts et notes de projet, suit ses tâches, veut tout garder en local.
- Petit studio (`studio`) : 3 à 10 personnes, partagent un dépôt Git, suivent projets et tâches dans des bases liées.
- Agent IA (`agent`) : Lit et écrit des pages, prend des tâches et rend compte, uniquement via MCP ou la CLI.

### Exigences fonctionnelles
- **EF-01** Espace de travail local dans un projet .craft — Créer, ouvrir et fermer un espace ; aucun compte ni réseau requis.
- **EF-02** Pages imbriquées en arborescence — Créer, renommer, déplacer, mettre à la corbeille, restaurer.
- **EF-03** Éditeur de blocs — Paragraphes, titres, listes, cases à cocher, citations, encadrés, code, séparateurs, bascules, images, tableaux simples.
- **EF-04** Liens entre pages et rétroliens — Mention d'une page, liste des pages qui pointent vers la page courante.
- **EF-05** Recherche plein texte — Dans les titres, les blocs et les propriétés ; palette de navigation rapide.
- **EF-06** Bases de données comme collections de pages — Base pleine page ou intégrée dans une page ; chaque ligne s'ouvre comme une page.
- **EF-07** Propriétés typées — Texte, nombre, case, date, URL, sélection, multi-sélection, personne (texte), relation, rollup, formule.
- **EF-08** Vues multiples — Tableau, kanban, liste, calendrier ; galerie et chronologie plus tard.
- **EF-09** Tri, filtre et groupement — Combinables, enregistrés par vue.
- **EF-10** Relations, rollups et formules — Relier deux bases, agréger, calculer avec un sous-ensemble de formules documenté.
- **EF-11** Gestion de tâches — Modèle de base « Tâches » : statut, échéance, assigné, priorité, liens vers projet ou commit.
- **EF-12** Toute action est une commande nommée — Accessible depuis l'UI, un raccourci, la CLI, le canal de contrôle et MCP, avec les mêmes paramètres.
- **EF-13** Annuler et rétablir — Sur toute commande qui modifie l'espace.
- **EF-14** Historique et synchronisation par Git — Diffs lisibles, fusion sans perte dans les cas courants, conflits signalés clairement.
- **EF-15** Import — Export Markdown + CSV du logiciel de référence ; dossier de fichiers Markdown.
- **EF-16** Export — Markdown, CSV, HTML, PDF.
- **EF-17** Pièces jointes et images — Copiées dans le projet, référencées par chemin relatif.

### Exigences non fonctionnelles
- **ENF-01** Local-first — Tout fonctionne hors ligne et sans compte.
- **ENF-02** Formats ouverts et fusionnables — Markdown et texte structuré ; un fichier modifié à la main est relu correctement.
- **ENF-03** Never-crash — Standard craftrules : aucun panic hors tests, entrées traitées comme hostiles, test de régression pour chaque crash.
- **ENF-04** Performance — Cibles à confirmer en conception : espace de 10 000 pages, base de 100 000 lignes, ouverture sous 1 s, recherche sous 100 ms.
- **ENF-05** Multiplateforme — macOS, Windows, Linux, BSD et web (wasm).
- **ENF-06** Clean-room et assets originaux — Rien de copié du logiciel de référence ; chaque asset dans ATTRIBUTION.md.
- **ENF-07** Licence — MIT ou Apache-2.0, comme le reste de la suite.
- **ENF-08** Accessibilité — Tout est faisable au clavier ; libellés pour les lecteurs d'écran.
- **ENF-09** Localisation — Chaînes d'interface via craft-i18n.
- **ENF-10** Qualité — cargo xtask ci vert à chaque fusion, clippy -D warnings, catalogue de parité généré.

### Contraintes
- Règles de la suite Craft (AGENTS.md, craftrules) et architecture en couches L0–L6.
- Séparation des salles : la salle B ne lit jamais plan/rea/ ni la salle A.
- Pas de code GPL/LGPL/AGPL, pas de lecture du code de projets concurrents.
- Aucune marque tierce dans les identifiants, fichiers ou textes d'interface.

### Risques
- Refus de l'usage de REA par storytold — Annoncer tôt (P0-07) ; garder chaque claim [rea] confirmé par une observation [obs].
- Conflit de nom NoteCraft — Faire acter le nom (P0-08) ; PageCraft en repli.
- Fusions Git difficiles sur les bases — Source de vérité en texte, SQLite en cache reconstructible (P3-02).
- Périmètre trop large — Lots courts avec démo de sortie ; lot 5 en backlog.
- Format .craft (#2131) pas encore défini — Isoler l'accès au projet derrière une interface (US-L0-03), adaptable plus tard.

## Use cases

### UC-01 — Créer et organiser des pages
Acteur : `solo` · Exigences : EF-01, EF-02
1. Ouvre son projet .craft dans NoteCraft.
2. Crée une page « Projet court-métrage » puis des sous-pages.
3. Glisse une sous-page sous une autre.
4. Met une page à la corbeille puis la restaure.
- Erreur : Nom de page déjà pris au même niveau : suffixe proposé, pas d'écrasement.

### UC-02 — Rédiger un document riche
Acteur : `solo` · Exigences : EF-03, EF-13, EF-17
1. Tape « / » pour insérer un titre, une liste, un encadré.
2. Colle une image, qui est copiée dans le projet.
3. Annule puis rétablit une modification.
- Erreur : Image illisible : bloc d'erreur visible, le reste de la page intact.

### UC-03 — Relier des notes entre elles
Acteur : `solo` · Exigences : EF-04
1. Mentionne une autre page dans un paragraphe.
2. Ouvre la page cible et voit le rétrolien.
3. Renomme la page cible ; le lien suit.
- Erreur : Page cible supprimée : lien marqué comme cassé.

### UC-04 — Retrouver une information
Acteur : `solo` · Exigences : EF-05
1. Ouvre la palette de recherche.
2. Tape quelques mots.
3. Ouvre le résultat, le passage est surligné.

### UC-05 — Créer une base et la remplir
Acteur : `studio` · Exigences : EF-06, EF-07
1. Crée une base « Plans de tournage » dans une page.
2. Ajoute des propriétés date, sélection, URL.
3. Saisit des lignes et ouvre l'une d'elles pour y écrire des notes.
- Erreur : Valeur invalide pour le type : cellule refusée avec message, rien n'est perdu.

### UC-06 — Analyser une base
Acteur : `studio` · Exigences : EF-08, EF-09, EF-10
1. Filtre les lignes de la semaine.
2. Groupe par statut et trie par date.
3. Ajoute une formule de durée.
4. Enregistre la vue.
- Erreur : Formule invalide : erreur indiquée dans la cellule et dans l'éditeur de formule.

### UC-07 — Relier deux bases
Acteur : `studio` · Exigences : EF-10
1. Crée une relation Projets ↔ Tâches.
2. Ajoute un rollup « % de tâches faites » côté Projets.
- Erreur : Ligne liée supprimée : relation nettoyée des deux côtés.

### UC-08 — Suivre des tâches en kanban
Acteur : `studio` · Exigences : EF-08, EF-11
1. Crée une base depuis le modèle Tâches.
2. Ouvre la vue kanban groupée par statut.
3. Glisse une carte de « À faire » à « En cours ».

### UC-09 — Planifier au calendrier
Acteur : `studio` · Exigences : EF-08, EF-11
1. Ouvre la vue calendrier sur la propriété échéance.
2. Déplace une tâche d'un jour à l'autre.
- Erreur : Ligne sans date : listée à part, non perdue.

### UC-10 — Un agent prend et traite une tâche
Acteur : `agent` · Exigences : EF-11, EF-12
1. Liste via MCP les tâches « À faire » qui lui sont assignables.
2. Prend une tâche (claim), qui passe « En cours » à son nom.
3. Ajoute un compte rendu dans le corps de la tâche et la passe « En revue ».
- Erreur : Tâche déjà prise par un autre : refus explicite, aucune écriture.

### UC-11 — Un agent rédige une page
Acteur : `agent` · Exigences : EF-03, EF-12
1. Crée une page via MCP avec du Markdown.
2. Met à jour une section sans toucher au reste.
- Erreur : Paramètres invalides : erreur structurée, jamais de crash (ENF-03).

### UC-12 — Versionner et synchroniser avec Git
Acteur : `studio` · Exigences : EF-14
1. Modifie des pages et une base.
2. Commit et pousse avec son outil Git habituel.
3. Un collègue tire, NoteCraft recharge les changements.
- Erreur : Conflit de fusion : signalé page par page avec les deux versions.

### UC-13 — Migrer depuis le logiciel de référence
Acteur : `solo` · Exigences : EF-15
1. Exporte son espace en Markdown + CSV depuis le logiciel de référence.
2. Importe l'archive dans NoteCraft.
3. Consulte le rapport d'import (ce qui est repris, ce qui est perdu).
- Erreur : Archive incomplète : import partiel avec rapport, rien d'écrasé.

### UC-14 — Exporter et partager un document
Acteur : `solo` · Exigences : EF-16
1. Exporte une page en PDF ou HTML.
2. Exporte une base en CSV.

### UC-15 — Travailler hors ligne puis fusionner
Acteur : `studio` · Exigences : EF-01, EF-14
1. Travaille sans réseau pendant un déplacement.
2. Fusionne au retour via Git ; les identifiants stables évitent les doublons.

## Lots et user stories

### L0 — Fondations (M0)
Un squelette Craft conforme qui ouvre un espace vide, exécute des commandes et passe la CI.

**Sortie :** notecraft ouvre un projet .craft vide, crée une page par la CLI et par MCP, annule l'opération, et cargo xtask ci est vert sur les trois OS et en wasm.

- [ ] **US-L0-01** Squelette du dépôt et cargo xtask ci — taille M — _À faire_
  - Étant donné un clone neuf, quand je lance cargo xtask ci, alors fmt, clippy -D warnings, tests, contrôle des assets et des couches passent.
  - Étant donné une crate de production, quand elle contient un unwrap, alors la CI échoue.
- [ ] **US-L0-02** Session, annuler/rétablir et registre de commandes — taille M — _À faire_ — UC-02, UC-11
  - Étant donné une commande enregistrée, quand je l'appelle par son id avec des paramètres valides, alors elle s'exécute et peut être annulée puis rétablie.
  - Étant donné des paramètres invalides, quand j'appelle une commande, alors j'obtiens une erreur structurée et l'état est inchangé.
- [ ] **US-L0-03** Créer et ouvrir un espace dans un projet .craft — taille M — _À faire_ — UC-01
  - Étant donné un dossier de projet, quand je crée un espace, alors un dossier notecraft/ et son manifeste sont créés.
  - Étant donné un manifeste corrompu, quand j'ouvre l'espace, alors une erreur claire s'affiche et rien n'est modifié.
- [ ] **US-L0-04** Identifiants stables — taille S — _À faire_ — UC-12, UC-15
  - Étant donné une page renommée ou déplacée, quand je la relis, alors son identifiant est inchangé.
  - Étant donné deux pages créées sur deux machines, quand on fusionne, alors leurs identifiants ne se chevauchent pas.
- [ ] **US-L0-05** CLI minimale — taille S — _À faire_ — UC-11
  - Étant donné un espace, quand j'exécute notecraft run page.create --title X, alors la page existe et la sortie est en JSON.
  - Étant donné une commande inconnue, quand je l'exécute, alors le code retour est non nul avec un message lisible.
- [ ] **US-L0-06** Serveur MCP minimal — taille M — _À faire_ — UC-10, UC-11
  - Étant donné le serveur MCP lancé, quand je liste les outils, alors chaque commande du registre apparaît avec son schéma.
  - Étant donné un appel MCP, quand la commande réussit, alors la même modification est visible dans l'app.
- [ ] **US-L0-07** Fenêtre egui vide, capture hors écran et build wasm — taille M — _À faire_
  - Étant donné le dépôt, quand je lance l'exemple snapshot, alors un PNG de l'interface est produit sans ouvrir de fenêtre.
  - Étant donné la CI, quand elle construit la cible wasm, alors le build réussit.
- [ ] **US-L0-08** Test « paramètres hostiles » — taille S — _À faire_
  - Étant donné toutes les commandes du registre, quand je les appelle avec des paramètres absurdes, alors aucune ne panique.

### L1 — Pages (M1)
Écrire, organiser, relier et retrouver des pages.

**Sortie :** Un utilisateur crée une arborescence de pages, rédige avec tous les blocs du lot, relie des pages, retrouve un passage par la recherche ; un agent fait de même via MCP ; les fichiers Markdown sont lisibles et éditables à la main.
**Dépend de :** L0

- [ ] **US-L1-01** Créer, renommer, mettre à la corbeille une page — taille S — _À faire_ — UC-01
  - Étant donné un espace, quand je crée une page, alors un fichier Markdown avec front matter apparaît.
  - Étant donné une page à la corbeille, quand je la restaure, alors elle revient à sa place avec son contenu.
- [ ] **US-L1-02** Arborescence de pages imbriquées — taille M — _À faire_ — UC-01
  - Étant donné deux pages, quand je glisse l'une sous l'autre, alors l'arbre et le disque reflètent la nouvelle place.
  - Étant donné un déplacement qui créerait une boucle, quand je le tente, alors il est refusé.
- [ ] **US-L1-03** Blocs texte : paragraphes et titres — taille M — _À faire_ — UC-02
  - Étant donné une page, quand je tape et change un paragraphe en titre, alors le Markdown enregistré contient le bon niveau de titre.
  - Étant donné une page, quand j'annule, alors le bloc revient à son état précédent.
- [ ] **US-L1-04** Listes et cases à cocher — taille S — _À faire_ — UC-02
  - Étant donné une liste, quand j'appuie sur Tab, alors l'élément est indenté.
  - Étant donné une case, quand je la coche, alors le fichier contient [x].
- [ ] **US-L1-05** Blocs citation, encadré, code, séparateur, bascule — taille M — _À faire_ — UC-02
  - Étant donné chaque type de bloc, quand j'enregistre et rouvre la page, alors le bloc est identique.
  - Étant donné un bloc code, quand je choisis un langage, alors la coloration s'applique.
- [ ] **US-L1-06** Images et pièces jointes — taille S — _À faire_ — UC-02
  - Étant donné une image collée, quand j'enregistre, alors elle est copiée dans le projet et référencée par chemin relatif.
  - Étant donné un fichier manquant, quand j'ouvre la page, alors un bloc d'erreur s'affiche sans crash.
- [ ] **US-L1-07** Menu « / » d'insertion — taille S — _À faire_ — UC-02
  - Étant donné un bloc vide, quand je tape « / » puis « tit », alors les types de titres sont proposés et insérables au clavier.
- [ ] **US-L1-08** Réordonner les blocs — taille S — _À faire_ — UC-02
  - Étant donné deux blocs, quand je glisse l'un ou utilise le raccourci de déplacement, alors l'ordre change et s'annule en une fois.
- [ ] **US-L1-09** Liens entre pages et rétroliens — taille M — _À faire_ — UC-03
  - Étant donné une mention de la page B dans A, quand j'ouvre B, alors A apparaît dans ses rétroliens.
  - Étant donné B renommée, quand je relis A, alors le lien pointe toujours vers B.
- [ ] **US-L1-10** Recherche plein texte et palette rapide — taille M — _À faire_ — UC-04
  - Étant donné un mot présent dans une page, quand je le cherche, alors la page sort en tête avec le passage surligné.
  - Étant donné un fichier modifié hors de l'app, quand je cherche, alors l'index en tient compte.
- [ ] **US-L1-11** Markdown aller-retour sans perte — taille M — _À faire_ — UC-12
  - Étant donné un fichier édité à la main dans un éditeur de texte, quand NoteCraft le relit, alors le contenu est conservé.
  - Étant donné une page ouverte puis enregistrée sans changement, quand je fais git diff, alors il n'y a aucune différence.
- [ ] **US-L1-12** Pages via MCP — taille S — _À faire_ — UC-11
  - Étant donné le serveur MCP, quand je lis une page, alors j'obtiens son Markdown et ses métadonnées.
  - Étant donné une mise à jour d'une section, quand je l'envoie, alors le reste de la page est inchangé.

### L2 — Bases de données (M2)
Des bases dont chaque ligne est une page, avec propriétés, vues tableau, relations et formules.

**Sortie :** Deux bases liées (Projets ↔ Tâches) avec rollup et formule, filtrées et triées dans une vue tableau enregistrée ; deux clones Git fusionnent des modifications de lignes différentes sans conflit.
**Dépend de :** L1

- [ ] **US-L2-01** Créer une base pleine page ou intégrée — taille M — _À faire_ — UC-05
  - Étant donné une page, quand j'insère une base intégrée, alors elle s'affiche dans la page et existe sur disque.
  - Étant donné une base, quand j'ajoute une ligne, alors une page correspondante est créée.
- [ ] **US-L2-02** Propriétés texte, nombre, case, date, URL — taille M — _À faire_ — UC-05
  - Étant donné une propriété nombre, quand je saisis du texte, alors la saisie est refusée avec un message.
  - Étant donné une propriété date, quand je choisis une date, alors elle est stockée au format ISO 8601.
- [ ] **US-L2-03** Sélection et multi-sélection — taille S — _À faire_ — UC-05
  - Étant donné une sélection, quand je crée une option, alors elle reçoit une couleur et devient disponible pour toutes les lignes.
  - Étant donné une option renommée, quand je relis les lignes, alors elles affichent le nouveau nom.
- [ ] **US-L2-04** Vue tableau éditable — taille M — _À faire_ — UC-05
  - Étant donné une vue tableau, quand j'édite une cellule au clavier, alors la valeur est enregistrée et annulable.
  - Étant donné une base de 10 000 lignes, quand je défile, alors l'affichage reste fluide.
- [ ] **US-L2-05** Ouvrir une ligne comme une page — taille S — _À faire_ — UC-05
  - Étant donné une ligne, quand je l'ouvre, alors je vois ses propriétés en haut et un corps éditable en blocs.
- [ ] **US-L2-06** Tri, filtre, groupement et vues enregistrées — taille M — _À faire_ — UC-06
  - Étant donné un filtre et un tri combinés, quand j'enregistre la vue, alors ils sont restaurés à la réouverture.
  - Étant donné une base, quand je crée deux vues, alors chacune garde ses propres réglages.
- [ ] **US-L2-07** Relations entre bases — taille M — _À faire_ — UC-07
  - Étant donné une relation Projets ↔ Tâches, quand je lie une tâche à un projet, alors le lien apparaît des deux côtés.
  - Étant donné une tâche supprimée, quand j'ouvre le projet, alors la relation est nettoyée.
- [ ] **US-L2-08** Rollups — taille S — _À faire_ — UC-07
  - Étant donné un rollup « pourcentage coché », quand je coche une tâche liée, alors le pourcentage du projet se met à jour.
- [ ] **US-L2-09** Formules (sous-ensemble documenté) — taille L — _À faire_ — UC-06
  - Étant donné une formule valide, quand je la saisis, alors chaque ligne affiche le résultat.
  - Étant donné une formule invalide, quand je la saisis, alors l'erreur est localisée et rien ne plante.
- [ ] **US-L2-10** Stockage fusionnable Git — taille L — _À faire_ — UC-12, UC-15
  - Étant donné deux clones qui modifient deux lignes différentes, quand on fusionne, alors il n'y a pas de conflit.
  - Étant donné le cache supprimé, quand j'ouvre la base, alors il est reconstruit à l'identique.
- [ ] **US-L2-11** Bases via MCP — taille S — _À faire_ — UC-10
  - Étant donné le serveur MCP, quand je requête une base avec un filtre, alors je reçois les lignes correspondantes en JSON.
  - Étant donné une écriture de propriété invalide, quand je l'envoie, alors j'obtiens une erreur structurée.

### L3 — Tâches et vues (M3)
Gérer des tâches en kanban, liste et calendrier, et laisser les agents prendre des tâches.

**Sortie :** Une équipe suit un sprint en kanban et au calendrier ; un agent liste, prend, met à jour et rend compte d'une tâche via MCP ; le suivi des US de NoteCraft lui-même peut migrer dans NoteCraft.
**Dépend de :** L2

- [ ] **US-L3-01** Modèle de base « Tâches » — taille S — _À faire_ — UC-08
  - Étant donné un espace, quand je crée une base depuis le modèle Tâches, alors elle a statut, échéance, assigné et priorité.
- [ ] **US-L3-02** Vue kanban — taille M — _À faire_ — UC-08
  - Étant donné une vue kanban groupée par statut, quand je glisse une carte, alors le statut change et l'action est annulable.
  - Étant donné une ligne sans statut, quand j'ouvre le kanban, alors elle est dans une colonne « Sans statut ».
- [ ] **US-L3-03** Vue liste — taille S — _À faire_ — UC-08
  - Étant donné une vue liste, quand je coche une tâche, alors son statut passe à « Fait ».
- [ ] **US-L3-04** Vue calendrier — taille M — _À faire_ — UC-09
  - Étant donné une vue calendrier sur l'échéance, quand je glisse une tâche, alors sa date change.
  - Étant donné des lignes sans date, quand j'ouvre la vue, alors elles sont listées à part.
- [ ] **US-L3-05** Lier une tâche à un élément .craft ou un commit — taille S — _À faire_ — UC-08
  - Étant donné un hash de commit, quand je le colle dans une tâche, alors il devient un lien vers le commit dans le dépôt local.
- [ ] **US-L3-06** Workflow agent : lister, prendre, mettre à jour, rendre compte — taille M — _À faire_ — UC-10
  - Étant donné une tâche libre, quand un agent la prend, alors elle passe « En cours » à son nom.
  - Étant donné une tâche déjà prise, quand un second agent tente de la prendre, alors il reçoit un refus explicite.
- [ ] **US-L3-07** Vue « Mes tâches » — taille S — _À faire_ — UC-08
  - Étant donné plusieurs bases de tâches, quand j'ouvre « Mes tâches », alors je vois celles qui me sont assignées, toutes bases confondues.
- [ ] **US-L3-08** Rappels d'échéance dans l'app — taille S — _À faire_ — UC-09
  - Étant donné une tâche qui arrive à échéance aujourd'hui, quand j'ouvre l'app, alors un rappel s'affiche.

### L4 — Import et export (M4)
Permettre la migration et le partage.

**Sortie :** Un espace exporté du logiciel de référence est importé avec un rapport ; une page part en PDF et une base en CSV.
**Dépend de :** L3

- [ ] **US-L4-01** Importer un export Markdown + CSV du logiciel de référence — taille L — _À faire_ — UC-13
  - Étant donné une archive d'export, quand je l'importe, alors pages, bases et liens sont recréés et un rapport liste ce qui est perdu.
- [ ] **US-L4-02** Importer un dossier de fichiers Markdown — taille M — _À faire_ — UC-13
  - Étant donné un dossier de notes Markdown avec liens wiki, quand je l'importe, alors les liens deviennent des liens de pages.
- [ ] **US-L4-03** Exporter en Markdown et CSV — taille S — _À faire_ — UC-14
  - Étant donné une base, quand je l'exporte en CSV, alors chaque propriété est une colonne lisible.
- [ ] **US-L4-04** Exporter en HTML — taille S — _À faire_ — UC-14
  - Étant donné une page avec images, quand je l'exporte en HTML, alors un dossier autonome est produit.
- [ ] **US-L4-05** Exporter en PDF — taille M — _À faire_ — UC-14
  - Étant donné une page, quand je l'exporte en PDF, alors la mise en forme des blocs est conservée.

### L5 — Backlog (M5+)
Idées notées, à prioriser après le lot 4. Pas d'US détaillées pour l'instant.

**Sortie :** —
**Dépend de :** L4

- [ ] **US-L5-01** Modèles de pages et de bases — taille M — _À faire_
- [ ] **US-L5-02** Vue galerie — taille M — _À faire_
- [ ] **US-L5-03** Vue chronologie — taille L — _À faire_
- [ ] **US-L5-04** Commentaires sur les blocs — taille M — _À faire_
- [ ] **US-L5-05** Historique de page basé sur Git — taille M — _À faire_
- [ ] **US-L5-06** Co-édition en temps réel (piste Etherpad évoquée sur #2134) — taille L — _À faire_
- [ ] **US-L5-07** Publication web statique d'une page — taille M — _À faire_
- [ ] **US-L5-08** Synchronisation hors Git — taille L — _À faire_

## Méthode

**DoR**
- Critères Given/When/Then et cas d'erreur écrits.
- US dont elle dépend déjà intégrées.
- Crates touchées et propriétaire indiqués par le lead tech.
- Taille S ou M.

**DoD**
- Tests d'acceptation verts, mutants tués.
- cargo xtask ci vert.
- Aucun panic hors tests ; régression testée pour chaque crash corrigé.
- Nouvelles commandes en CommandSpec, présentes dans la parité.
- Revue approuvée et audit clean-room passé.
- Capture jointe si l'interface change.
- Statut mis à jour ici et docs Craft à jour.

**Rôles**
- Product owner (Tristan) : Valide les portes, accepte les lots, pousse et publie.
- Orchestrateur (Session Claude Code principale) : Choisit les US prêtes, lance les sous-agents, tient les statuts à jour, remonte les arbitrages.
- Spec (Session séparée (salle A)) : Seul à utiliser REA ; écrit et amende spec/.
- lead-tech (Sous-agent) : Architecture, ADR, fichier de story autosuffisant, attribution des crates.
- tester (Sous-agent) : Tests d'acceptation avant le code, mutants pour les prouver.
- dev (Sous-agent, un par US, worktree isolé) : Implémente jusqu'à ce que les tests passent et que la CI soit verte.
- reviewer (Sous-agent, lecture seule) : Relit le diff contre la spec et la checklist Craft.
- cleanroom-auditor (Sous-agent, lecture seule) : Contrôle le mur : marques, termes REA, textes copiés, assets.
- integrator (Sous-agent) : Fusionne en série dans develop, met à jour les docs Craft et les statuts.
