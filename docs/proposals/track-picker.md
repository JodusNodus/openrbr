# Proposal: stage and car browsers with favourites

**Status:** the stage browser and the favourites on the Drive page are built (`crates/rbr-launcher/src/stage_browser.rs`);
the car browser is next. Decisions: Choose stage / Choose car goes back to the Drive page, no shortcut for opening a
browser, the chips show favourites only. Differences from the mockups: a stage that cannot start is listed under
**Cannot start** (and last in the favourites, the recent picks and a search) and not in the other lists; the country
names come from RSF's ISO codes; the detail panel shows the weather of the good and bad presets and whether a ghost is
kept, not a best time; Random only moves the highlight.

Draft GUI proposal. The mockups are drawn with the launcher's own egui theme at its 1100 × 760 window size
(`crates/rbr-launcher/src/mockup.rs`, throwaway). They're in `out/track-picker/`, which is gitignored: the stage
pictures and car renders come from the game data, so they must not be committed.

## The problem

Today the Stage and Car fields open dropdowns (`today-drive-maps.png`):

- it's 360 pt wide and shows about 13 rows, while a full RSF install has about 450 stages;
- it covers the Drive page, the stage preview is the only picture, and you can't see much at once;
- recent picks are the only shortcut, and they get pushed out after 6;
- retail and RSF stages from the same country sit in separate groups;
- the retail cars show their raw names (`PEUGEOT_206`, `COROLLA`), and cars RSF hasn't downloaded are hidden behind
  a checkbox.

## The design

Picking a stage or a car is a decision you browse for. Changing between a few favourites is a quick action. They get
different UI. Stages and cars use the same browser component: only the collections, filters, columns and detail panel
differ.

### 1. Drive page: favourites one click away

![Drive page](../../out/track-picker/mockup-3-drive-page.png)

- The Car and Stage fields show the selection with a ★ if it's a favourite, and **Browse…** on the right. A click
  opens that browser.
- Under each is a row of favourite chips. One click selects without opening anything. The current selection's chip
  is highlighted. If the chips don't fit, the rest go behind "+3", which opens the browser on Favourites.
- The car chips take the place of **Add cars…**, which moves into the car browser.
- With no favourites a row isn't shown, and the page looks like today's apart from Browse….

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

- Finland has the 1.02 stages and the RSF ones together. The third column becomes the author (Warthog for the 1.02
  stages).
- A row's ☆ shows faintly, and brightens on hover (Lahti Night). A click on the star toggles the favourite and
  doesn't select the row.

### 4. Car browser: Favourites

![Car favourites](../../out/track-picker/mockup-4-car-favourites.png)

- **Left:** Favourites, Recent and All cars, then the classes, newest to oldest (WRC 2.0 down to Group 2/RGT). The
  1.02 cars sit in their real classes, not under "Retail". Classes RSF splits finely are merged (Rally 2 / R5, R2 / R3,
  Group A6/A7/A8 as Group A). **Not downloaded** comes last, and **Add cars…** sits under it.
- **Middle:** a Drive filter (4WD, FWD, RWD) and "only cars that can start". The columns are star, car, class, and
  model: a green dot for its own model, an amber dot for a stand-in.
- **Right:** the car turning, as the Drive page preview renders it today. Then the name, class and source, the saved
  setups, **★ Favourite** and **Choose car**.
- Search covers the name, the class and the RSF number.

### 5. Car browser: a class

![Car class](../../out/track-picker/mockup-5-car-class.png)

- Inside a class, the third column becomes the source (RSF number, RBR 1.02, Added).
- A car RSF hasn't downloaded stays in its class, greyed at the end with "not downloaded", so you can see what the
  class has. It can't be chosen. Its row shows how to get it on hover.

### Interaction

| Action | Mouse | Keyboard |
| --- | --- | --- |
| Open a browser | the field / Browse… | none (see decisions) |
| Search | the search box | typing anywhere |
| Move | hover | ↑ ↓, Page Up/Down |
| Preview | hover pauses, as today | the highlighted row |
| Choose and go back | double-click a row, or Choose stage / Choose car | Enter |
| Favourite | the star on a row or the detail button | Ctrl/Cmd+D |
| Back without changing | ⏴ Drive | Esc |

- A browser opens on the current selection's collection with it highlighted. With no favourites it opens on All, with
  a single hint in the empty Favourites: "Click ☆ next to a stage to keep it here."
- Favourites keep the order they were starred in. In the browser they sort like any other list. The chips on the
  Drive page use the starred order.
- Favourites are saved as `favourite_maps` (map numbers, like `recent_maps`) and `favourite_cars` (names, like
  `recent_cars`). A favourite that isn't in the catalog (RSF folder unset, pack removed) stays saved and just
  doesn't show.
- The Championship page's car list uses the same car browser.

### Data notes

- **Your best:** the best ghost already kept per map (`NGP/GhostCars/Best_<map>.ghc`, `ghosts.rs`). If its header
  has the time, show it. Otherwise show "driven" or "not driven yet". The times in the mockups are made up.
- **Weather** is the stage's season rows, which the launcher already reads for availability.
- **Car names and classes for the 1.02 cars** need a small table (`ZR` → MG ZR S1600, Super 1600). The RSF cars
  already have their names and classes.
- **Drive filter:** each car's `DriveType` in its physics (`lsp_keys.rs`). It has to be read for every car when the
  catalog is built. If that turns out too slow, the filter is left out.
- **Setups** are the saved setups already listed per car (`Physics/<car>/Setups`).
- The RSF stage names, authors and counts in the mockups are invented, as are the class counts. The retail names,
  lengths and surfaces, the RSF car names and classes, and the car renders are real.

## Why this over a better dropdown

- A list of 450 stages needs room. The full content area holds about 14 rows with 5 readable columns plus a large
  picture, instead of 13 cramped rows.
- Favourites become a first-class collection with a one-click path from the Drive page, not just another group in a
  list.
- It's the pattern players already know from Content Manager and RSF's own launcher: collections on the left, list in
  the middle, details on the right.
- The Drive page gets simpler, not busier: per field, one button and an optional chip row, and Add cars… moves out.

## Decisions for you

1. Should Choose stage / Choose car go back to the Drive page, or stay in the browser and update the selection? The
   mockups assume it goes back.
2. Is it worth a shortcut for the browser (Ctrl+K or F)? I'd leave it out unless you want it.
3. The chips on the Drive page: favourites only (as drawn), or favourites then recent when there are few favourites?
