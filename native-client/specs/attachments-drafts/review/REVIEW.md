# 2.7 Attachments + 2.8 Drafts: design review, round 1

Test server: `tools/test-server.sh 4814` with Second chat and Narrow chat added. Web: a hidden browser at 1400×860.
Native: testctl 4815, the same layout (wide on top; under it a wide pane and a narrow one, 60/40).
In the web, uploads were driven through a stubbed XHR (progress 42% / 18%, then "saving", then the real server answer).
In the native client they are real uploads (120 MB files, so the bar can be caught mid-way).
Every crop is web on top, native below.

| State | Crop | Verdict |
|---|---|---|
| uploading, wide | progress-wide-* | parts, order and colours match. Track not centred on the text (nit) |
| saving | saving-* | matches |
| attached (path at the caret, spacing, toast) | attached-* | matches. Web and native both leave the caret before an existing space |
| uploading, narrow, long name | progress-narrow-long-name-* | **differs**: the native name isn't ellipsized and the track shrinks to about 50px |
| drop outline (wide, narrow, over the header, gone) | dropping-second-*, dropping-narrow-native-*, native-drop-header-then-gone | matches. The header drop is the named difference |
| refused upload (red toast) | error-refused-* | matches |
| drafts after a restart | drafts-after-restart-* | matches. The first chat built adopts the project copy, in the web too (checked by reload) |

Behaviour, checked by hand:
- **Real clipboard (arboard helper on :99):**
  - Ctrl+V of a picture uploads image.png, a valid 64×48 PNG;
  - Ctrl+V of a copied file uploads it under its own name;
  - Ctrl+V of text pastes the text;
  - Shift+Insert of text pastes nothing (nit).
- **Restart:** drafts survive it, kept per server under `chat:<id>`, plus `project:demo`.
- **Adoption:** a new chat opened while the owner is still open starts empty; after the owner ends, the next new chat adopts its text.
- **Disk writes:** `session.json` is rewritten once per keystroke (17 writes for 18 keys typed 155 ms apart), not at most every menu-linger (nit).
- **Real click on Attach without a portal (Xvfb, no D-Bus):** nothing opens and the UI stays responsive. This is the named gap.

Problems found:
1. **(blocking)** Upload bar, long file name. Web: name ⅓, ellipsized ("quarterly-report-final…"), track ⅔. Native: the whole name is shown and the track is about 50px wide. In wide panes the shares are off too: the track starts about 25px later.
2. **(blocking)** The composer's "line above" (`ui/chat.slint`, `Rectangle { height: 1px }` with no `y`) is centred in the composer. It runs through the input row and buttons, moves as the upload bar shows, and is missing at the composer's top edge where the web has its border. It is visible in every crop: progress-wide, drafts-after-restart and the narrow pane.
3. (nit) The track sits about 4px above the text's centre. The web centres it.
4. (nit) The draft write throttle isn't a throttle (`write_drafts_soon` starts a timer per change).
5. (nit) Shift+Insert doesn't paste text. Slint's TextInput only knows Ctrl+V.
6. (nit) PLAN 2.7 still mentions "the chips", and the visual-only list still names "attachments".

Mode picker F50–F52, re-checked:
- F50: hovering a pane under the scrim shows no split controls; after closing, the same position shows them. See `mode-picker/review/F50-*-r2.png`.
- F51: in a 400px-high window, Tab keeps the focused option or × in view (PickerProbe: in view at every step).
- F52: the thumb shows. See `mode-picker/review/F51-*-r2.png`.
