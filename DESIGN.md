---
name: Remote Rebirth
description: Owner's-manual desktop configuration for the Logitech Harmony 300.
colors:
  paper: "#F6F0E3"
  paper-bright: "#FBF7ED"
  desk: "#E3D8C4"
  ink: "#23211C"
  muted: "#6E6455"
  faint: "#766B5B"
  rule: "#D2C6B0"
  oxide: "#B34824"
  oxide-deep: "#8E3A1F"
  oxide-wash: "#F0DDD2"
  grass: "#5B7153"
  cobalt: "#2F5A7A"
  cloth: "#8C3F25"
  cloth-deep: "#73301C"
  cloth-text: "#EED0BC"
  cream: "#FFF7E8"
  hover: "#EFE6D4"
  pressed: "#E7DCC8"
typography:
  display:
    fontFamily: "Source Serif 4, Georgia, serif"
    fontSize: "31px"
    fontWeight: 600
    lineHeight: 1.05
    letterSpacing: "-0.025em"
  headline:
    fontFamily: "Source Serif 4, Georgia, serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: 1.2
  title:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 600
    lineHeight: 1.3
  body:
    fontFamily: "Public Sans, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: "Spline Sans Mono, ui-monospace, monospace"
    fontSize: "8.5px"
    fontWeight: 500
    lineHeight: 1.2
    letterSpacing: "0.04em"
rounded:
  none: "0px"
  remote: "18px"
spacing:
  "2xs": "4px"
  xs: "6px"
  sm: "8px"
  md: "14px"
  lg: "18px"
  xl: "24px"
components:
  button-primary:
    backgroundColor: "{colors.oxide}"
    textColor: "{colors.cream}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    padding: "6px 12px"
  button-primary-hover:
    backgroundColor: "{colors.oxide-deep}"
    textColor: "{colors.cream}"
    rounded: "{rounded.none}"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    padding: "6px 10px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.title}"
    padding: "4px 6px"
  navigation-rail:
    backgroundColor: "{colors.cloth}"
    textColor: "{colors.cream}"
    width: "216px"
  nav-item-active:
    backgroundColor: "{colors.cloth-deep}"
    textColor: "{colors.cream}"
    typography: "{typography.title}"
    padding: "7px 7px"
  tab-active:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    height: "32px"
  input:
    backgroundColor: "{colors.paper-bright}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "6px"
  sheet:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
  device-bay:
    backgroundColor: "{colors.paper-bright}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "8px 9px"
  key:
    backgroundColor: "{colors.paper-bright}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    height: "20px"
  stamp:
    backgroundColor: "transparent"
    textColor: "{colors.grass}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "2px 5px"
  phase-rail:
    backgroundColor: "{colors.oxide-wash}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    width: "74px"
---

# Design System: Remote Rebirth

## Overview

**Creative North Star: "The Open Owner's Manual"**

The GUI is a printed owner's manual that happens to be interactive. A cloth-bound rail holds the chapters; a single uncoated sheet carries the work; every page is set as a spread with a fixed header, a page number, and hairline rules instead of floating cards. The remote itself is explained once, as a numbered line drawing, and that drawing is the spine of the key editor.

The system is calm because the task is cautious: the owner is about to erase and rewrite a device they depend on. Paper, ink, and one oxide spot carry the action; grass confirms, cobalt marks inheritance, and oxide-deep carries failure. Technical values appear only as mono marginalia, never as the page's voice.

**Key Characteristics:**
- Paper-and-cloth surfaces with a single ambient sheet shadow.
- One oxide action spot; success and inheritance are separate, labelled states.
- Numbered callouts tie the remote drawing to the work table.
- Serif headings, sans work text, mono codes.
- Flat, square controls; one 18px radius only on the schematic remote body.

## Colors

A warm paper field with a cloth rail, one oxide spot, and two quiet state inks. The palette is light because the owner works in a living room, often with the remote in hand, and reads the screen at arm's length.

### Primary
- **Oxide Spot** (#B34824): the primary action, the selected key, the selected device bay, the active phase, and the callout numbers. Its rarity is the point.
- **Oxide Deep** (#8E3A1F): destructive labels, error text, and the primary action's hover/pressed state.
- **Oxide Wash** (#F0DDD2): selected rows, active bays, and error slips. It is a ground, never a text colour.

### Secondary
- **Grass Confirmed** (#5B7153): a verified backup, an assigned key, a completed state.
- **Cobalt Inherited** (#2F5A7A): a key supplied by another device through Routing.

### Neutral
- **Uncoated Paper** (#F6F0E3): the sheet and the page ground.
- **Bright Paper** (#FBF7ED): inputs, key faces, and tab faces.
- **Desk** (#E3D8C4): the workspace around the sheet.
- **Ink** (#23211C): body and headings.
- **Muted Ink** (#6E6455): secondary text at 5.1:1 on paper.
- **Faint Ink** (#766B5B): tertiary text and placeholders at 4.6:1 on paper.
- **Rule** (#D2C6B0): every hairline, border, and gutter.
- **Cloth** (#8C3F25): the navigation rail.
- **Cloth Deep** (#73301C): the active chapter.
- **Cloth Text** (#EED0BC): rail secondary text at 5.0:1 on cloth.
- **Cream** (#FFF7E8): text on oxide and cloth.

### Named Rules
**The One Spot Rule.** Oxide appears only on something the owner can act on or has selected. It never decorates a neutral surface, and it never carries a state that words can carry alone.

**The Paper Ground Rule.** Cards are not containers here. A region is paper, a rule, or a marginal note; nothing floats on a second white surface.

## Typography

**Display Font:** Source Serif 4 SemiBold (with Georgia, serif)
**Body Font:** Public Sans (with system-ui, sans-serif)
**Label/Mono Font:** Spline Sans Mono (with ui-monospace, monospace)

**Character:** A bookish serif gives the chapters a printed voice; a plain humanist sans keeps the work legible; a narrow mono is reserved for protocol names, digests, paths, counts, and the numbered callouts.

### Hierarchy
- **Display** (600, 31px, 1.05, -0.025em): page titles only.
- **Headline** (600, 18px, 1.2): section headings and the command index title.
- **Title** (600, 11.5px, 1.3): button labels, field labels, device names in bays.
- **Body** (400, 11.5px, 1.4, 65–75ch where prose runs): instructions and explanations.
- **Label** (500, 8.5px, 1.2, uppercase where it is a marginal note): section notes, counts, page numbers, stamp labels, callout numbers.

### Named Rules
**The Callout Rule.** Every key in the drawing carries a two-digit mono number in oxide; the same number opens the matching table row. The drawing and the table are one document, not two views.

**The Mono Margin Rule.** Mono is for values, never for prose. If a sentence moves into mono, it is a code, a path, a count, or a callout.

## Layout

The window is a desk: a 216px cloth rail on the left, a 14px desk margin, and a paper sheet that fills the rest. The sheet has a fixed header (24px side padding, 18px top), a fixed footer (page number, save state), and one scrolling body. The machine status strip sits outside the sheet on the desk, 30px high.

The body uses two-column spreads. Remote and Devices are a 2:3 split with a 1px gutter; Buttons is a 318px figure column, a 1px gutter, and a flexible work column; Settings is a balanced two-column spread. Spacing steps are 4, 6, 8, 14, 18, and 24px. Sections are separated by an 18px gap and a 1px rule, never by a card.

The default window is 1240×820; the supported minimum is 1040×680. The configured Model opens on Buttons; a Model with no Devices opens on Remote so detection and backup come first.

## Elevation & Depth

The system is flat paper. Depth comes from the sheet sitting on the desk, one 1px rule weight, and the ordering of paper, ink, and marginal notes. Buttons, tabs, inputs, and stamps are flat at rest and change fill or border on hover, pressed, and disabled.

### Shadow Vocabulary
- **Sheet ambient** (`0 10px 28px rgba(38, 31, 22, 0.18)`): the only shadow in the system; it lifts the paper sheet off the desk.
- **No control shadow**: buttons, inputs, keys, tabs, and stamps never cast one.

### Named Rules
**The One Sheet Rule.** Only the sheet may cast a shadow. A shadow on a control means the surface has stopped being paper.

## Shapes

The form language is a printed page: square corners, 1px rules, and crisp vector geometry. The sheet, buttons, inputs, tabs, keys, stamps, and bays have 0 radius. The only exception is the schematic remote body at 18px, because it is a drawn object rather than a page element. Status dots are 7px circles. There are no pills, no soft cards, and no coloured side stripes.

## Components

### Buttons
- **Shape:** square (0 radius), 1px border.
- **Primary:** oxide fill, cream label, 6×12 padding; hover and pressed deepen to oxide-deep; disabled uses the pressed paper fill and faint ink.
- **Secondary:** transparent with a 1px rule border and ink label; hover fills with the hover paper; pressed darkens to the pressed paper.
- **Ghost:** no border; hover fills with the hover paper. Used for table-row actions.
- **Danger:** transparent with a rule border and oxide-deep label; hover fills oxide-wash and borders oxide.

### Navigation
- **Rail:** 216px cloth ground; brand in 21px serif cream, subtitle in 8px mono cloth-text; chapters in 12.5px Public Sans.
- **Active chapter:** cloth-deep fill, cream semibold label, cream count. No side stripe.
- **Counts:** 8px mono, cloth-text at rest and cream when active.

### Tabs
- **Device tabs:** 32px high, 1px rule border, 0 radius, 10.5px semibold label plus an 8px mono count; active uses paper fill and ink border; inactive is transparent with muted text.

### Device Bays
- **Style:** bright paper fill, 1px rule border, 0 radius, 8×9 padding; the active bay uses oxide-wash with an oxide border and oxide-deep labels; the name is 14px serif.
- **Empty bay:** transparent with a faint label and "Empty slot"; it never pretends to be a device.

### Key Diagram
- **Body:** 288px wide, bright paper, 1px ink border, 18px radius, 6px padding; a dark ink screen with cream device/command text.
- **Keys:** 20px high with the callout number and short label on one line, 0 radius; assigned keys use bright paper with an ink border, empty keys use a rule border and faint label; the selected key is oxide with cream text; inherited keys use bright paper with cobalt text.
- **Callouts:** 6.5px mono oxide numbers leading every key; the number matches the table row.

### Inputs and Selects
- **Inputs:** bright paper, 1px rule border, 0 radius, 5–6px padding, 10–11px Public Sans or mono for paths; focus switches the border to oxide; placeholder uses faint ink.
- **Selects:** bright paper, 1px rule border, 0 radius, ink text and muted handle; the menu is bright paper with an ink border and an oxide selected row.

### Sheet and Page
- **Sheet:** paper fill, 1px rule border, 0 radius, one ambient shadow.
- **Header:** display title, one-line body subtitle, chapter number and identity in mono, actions on the right.
- **Footer:** 8px mono colophon on the left, save state in the centre, page number on the right; 1px rule above.

### Stamps
- **Style:** transparent fill, 1px border in the state colour, 0 radius, 2×5 padding, 7.5px mono uppercase label. Verified is grass, Invalid is oxide-deep, Unchecked is faint. The word is always present; colour is secondary.

### Phase Rail
- **Style:** 74px fore-edge rail; cells are sized by the work they hold (2/3/3/2/2 portions), separated by 1px rules; the active cell uses oxide-wash with an oxide border, oxide-deep number, ink label, and a filled oxide dot. Labels: Detect, Backup, Prepare, Commit, Readback.

## Do's and Don'ts

### Do:
- **Do** keep the sheet as the only shadowed surface and use 1px rules for structure.
- **Do** pair every state colour with its word: Verified, Invalid, Assigned, Inherited, Empty.
- **Do** keep the callout number on every key and the matching number in the table.
- **Do** use the serif for page and section headings and Public Sans for work text.
- **Do** use mono only for codes, paths, counts, and callouts.
- **Do** keep the configured Model opening on Buttons and the empty Model on Remote.

### Don't:
- **Don't** put controls on cards or add a second white surface.
- **Don't** add a coloured side stripe to a nav item, row, or callout.
- **Don't** use gradient text, glass, glyph icons, or emoji as iconography.
- **Don't** use blue as the action colour; oxide is the one spot.
- **Don't** round primary actions into pills; only the remote body is 18px.
- **Don't** ask the owner to type a command name where a pick list can offer the device's own commands.
