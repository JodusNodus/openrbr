# Proposal: a stage browser with favourites

Draft GUI proposal. The mockups are drawn with the launcher's own egui theme at its 1100 × 760 window size
(`crates/rbr-launcher/src/mockup.rs`, throwaway). They're in `out/track-picker/`, which is gitignored: the stage
pictures come from the game data, so they must not be committed.

## The problem

Today the Stage field opens a dropdown (`today-drive-maps.png`):

- it's 360 pt wide and shows about 13 rows, while a full RSF install has about 450 stages;
- it covers the Drive page, the stage preview is the only picture, and you can't see much at once;
- recent picks are the only shortcut, and they get pushed out after 6;
- retail and RSF stages from the same country sit in separate groups.

## The design

Picking a stage is a decision you browse for. Changing between a few favourites is a quick action. They get
different UI.

### 1. Drive page: favourites one click away

![Drive page](../../out/track-picker/mockup-3-drive-page.png)

- The Stage field shows the selection with a ★ if it's a favourite, and **Browse…** on the right. A click opens the
  stage browser.
- Under it is a row of favourite chips. One click selects a stage without opening anything. The current stage's chip
  is highlighted. If the chips don't fit, the rest go behind "+4", which opens the browser on Favourites.
- With no favourites the row isn't shown. The Drive page looks like today's, apart from Browse….

### 2. Stage browser: Favourites

![Favourites](../../out/track-picker/mockup-1-favourites.png)

The browser replaces the Drive page's body. The tabs stay, and "⏴ Drive" or Esc goes back without changing the
selection.

- **Left: collections.** Favourites, Recent and All stages, then the countries with their counts, with retail and RSF
  stages together. Stages that can't start are collected last, together with the reason for each.
- **Middle: the list.** Surface chips, a Length menu, Sort (name, length, recently driven). The columns are star,
  name, country (the author inside a country), length, and surface with a colour dot. Rows are 30 pt, about 14 visible
  without scrolling.
- **Right: the stage.** Picture, name, country and origin, length and surface chips, your best time if there is one,
  and the weathers. Below them are **★ Favourite** and **Choose stage**.
- The search box at the top searches everything (name, country, author, map number), whatever collection is
  selected. The results are grouped by country, with favourites first.
- **Random** picks from the list as currently filtered, so "random gravel favourite" works.

### 3. Stage browser: a country

![Country](../../out/track-picker/mockup-2-country.png)

- Finland has the 1.02 stages and the RSF ones together. The third column becomes the author (Warthog for the 1.02 stages).
- A row's ☆ shows faintly, and brightens on hover (Lahti Night). A click on the star toggles the favourite and
  doesn't select the row.

### Interaction

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Open the browser | the Stage field / Browse… | Ctrl+K? (see decisions) |
| Search | the search box | typing anywhere |
| Move | hover | ↑ ↓, Page Up/Down |
| Preview | hover pauses, as today | the highlighted row |
| Choose and go back | double-click a row, or Choose stage | Enter |
| Favourite | the star on a row or the detail button | Ctrl/Cmd+D |
| Back without changing | ⏴ Drive | Esc |

- The browser opens on the current stage's collection with the stage highlighted. A first start with no favourites
  opens on All stages, with a single hint in the empty Favourites: "Click ☆ next to a stage to keep it here."
- Favourites keep the order they were starred in. In the browser they sort like any other list. The chips on the
  Drive page use the starred order.
- Favourites are saved as `favourite_maps` (map numbers, like `recent_maps`). A favourite RSF stage stays saved when
  the RSF folder isn't set, and it just doesn't show.

### Data notes

- **Your best:** the best ghost already kept per map (`NGP/GhostCars/Best_<map>.ghc`, `ghosts.rs`). If its header
  has the time, show it. Otherwise show "driven" or "not driven yet". The times in the mockups are made up.
- **Weather** is the stage's season rows, which the launcher already reads for availability.
- The RSF names, authors and counts in the mockups are invented. The retail names, lengths and surfaces are real.

## Why this over a better dropdown

- A list of 450 stages needs room. The full content area holds about 14 rows with 5 readable columns plus a large
  picture, instead of 13 cramped rows.
- Favourites become a first-class collection with a one-click path from the Drive page, not just another group in a
  list.
- It's the pattern players already know from Content Manager and RSF's own launcher: collections on the left, list in
  the middle, details on the right.
- The Drive page gets simpler, not busier: one field and an optional chip row.

## Decisions for you

1. Cars: give them the same browser and favourites now, or later? It's the same component, with the car model turning
   in the right-hand panel.
2. Should Choose stage go back to the Drive page, or stay in the browser and update the selection? The mockups assume
   it goes back.
3. Is it worth a shortcut for the browser (Ctrl+K or F)? I'd leave it out unless you want it.
4. The chips on the Drive page: favourites only (as drawn), or favourites then recent when there are few favourites?
