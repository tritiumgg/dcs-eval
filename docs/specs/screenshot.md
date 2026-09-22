# The screenshot capability, specified as an addition to a shipped build

This document is written the way `bridge.md` and `mcp.md` were: before the code, as the record of
what was intended. It is frozen when it lands. The build will drift from it, and where it does, a
decision record says so.

It specifies one capability: a tool and a command-line verb that take a screenshot inside a running
DCS and hand back the file. It changes nothing on the wire. The executor keeps its two ops, the
protocol keeps its headers, and the installer is untouched.

**Vocabulary.** *Executor* is the Lua file DCS loads, which the two shipped documents call the
bridge (ADR 0002). *The capture* is one call to DCS's own screenshot function. *The file* is what
DCS writes because of it.

---

## 0. What this is, and what it is not

**One sentence.** A seventh tool, `dcs_screenshot`, and a fifth command-line verb, `screenshot`,
that name a file, ask DCS for a capture, wait until that file is written whole, and answer with its
path and size.

**Why this is a tool and not a chunk the caller ships.** `mcp.md` §0 draws the scope line, and it
is narrow on purpose:

> **Scope, deliberately narrow.** Evaluating Lua, and knowing the game's state. Not census, not
> collection, not modelling: a project that wants those ships its own Lua and evaluates it through
> this server, which is what `bridge.md` §7.0 made of `census` and `reflect`.

`census` and `reflect` went because each was `eval` wearing a hat: the Lua was the whole of them,
and a consumer can ship Lua. The Lua here is one line. Everything that makes a screenshot usable
sits outside DCS, where a chunk cannot reach it: choosing a name DCS will not silently reject,
knowing that the file on disk is the capture that was just asked for and not last week's under the
same name, knowing that it is complete rather than half-encoded, and reading back its size. Those
are the client's work, they are the same work every time, and today every caller writes them again
by hand. That is what earns a tool.

**What it is not.** Not a camera: where the camera points, whether the game is paused and what is
on screen are the caller's, through `eval`, before this is called. Not an image library: the file
DCS writes is handed back unmodified. Not a gallery: nothing here deletes, renames or tidies
anything.

**The count of tools changes.** `mcp.md` §2.3 is headed "The six tools" and its table carries six
rows. That heading is a fact about the build the day it was written. This document makes it seven,
and the README, the plan and the test that names them each say seven.

---

## 1. What DCS provides

### 1.1 The call

`DCS.makeScreenShot(name)`, in the `hook` state where the executor's own host lives, and in `gui`.
ED's API notes give it under the `Sim.` prefix they use for that table, with one sentence:
"Makes screenshot with given name" (`API/Sim_ControlAPI.md`). No file shipped with DCS calls it.

`DCS.setScreenShotExt(ext)` sets the format. DCS itself calls it in one place, when the options
dialog is confirmed (`MissionEditor/modules/me_options.lua:135`). Nothing reads the setting back.

`net.screenshot_request(player_id)` and `net.screenshot_del(player_id, key)` are a different
capability: a server asking a connected *client* for a picture, used by the web admin page
(`Scripts/Hooks/webGUI.lua`), gated by a server setting and a client setting. They are not this
document's subject and nothing here uses them.

The key binding is `iCommandScreenShot` (`Config/Input/UiLayer/keyboard/default.lua`). Its command
number is not in any Lua file the install ships, so driving the capture through `LoSetCommand`
would rest on a number nobody can point at. This document does not use it.

### 1.2 The file

| | |
|---|---|
| Directory | `<lfs.writedir()>\ScreenShots\`, that spelling |
| Named | `<name>.<ext>` for a name given, `Screen_YYMMDD_HHMMSS.<ext>` for an empty one |
| Format | `jpg`, `png` or `bmp`, from the user's options. The two files DCS ships disagree about the default: `MissionEditor/data/scripts/options.lua` says `jpg`, `Config/graphics.lua` says `png` beside `ScreenshotQuality = 90`. The machine measured here is `png`, in its own `Config/options.lua`, and every capture on it is a `.png` |
| Size | the full render resolution. On the machine this was written from, 2560x1440, and 256 KB to 7.5 MB a file — the small end is a flat map view, the large end a cockpit |

The picture is what was on screen, the game's own interface included: a message drawn by a mission
and an open radio menu both appear in captures taken this way. Nothing found in the install hides
the interface or the labels for a capture, so a caller who wants them gone turns them off itself
before asking.

### 1.3 What the call does not tell you

It returns nothing, and it does not write the file before it returns. Fourteen captures DCS named
itself carry the second they were asked for in the name and a modified time 0.38 s to 1.58 s after
it, which is the closest thing to a figure for the lag anyone here has. Two bursts on 2026-09-16
each hold a file stamped *before* one requested earlier — 88 ms and 85 ms before — and both of
those are the 0-byte files of the paragraph below, so what they date is the moment a capture was
abandoned rather than one finishing out of turn. Among the captures that produced a picture,
nothing seen here arrived out of order. The encoding finishes on some later frame; how much later,
measured rather than inferred from fourteen filenames, is §4's question.

So there is no callback, no returned path, and no completion signal of any kind. What the file will
be called is the only thing knowable in advance, and only if the caller supplies the name.

**A dot in the name loses the picture.** Four captures asked for on 2026-09-16 under names like
`verify_A01_shot9_c_plus0.3s` are on disk as 0-byte files carrying the name exactly as it was
asked for, dot and all, and no extension at all. DCS appears to read the text after the last dot
as the format, not know it, and write nothing. Nothing reports this: the call returns exactly as
it does when it works.

### 1.4 Where each of these comes from

| Fact | Evidence |
|---|---|
| The call, its states, its one-line description | the API index, and ED's own notes in the install |
| ED calls it nowhere; the ext setter's one caller | a search of the install's Lua |
| The directory, the default name, the resolution, the sizes | 62 files in one maintainer's `ScreenShots` |
| The format and its default | `Config/graphics.lua`, `options.lua` shipped and the maintainer's |
| The file arriving later, and out of order | timestamps on those files, to the microsecond |
| A dotted name losing the picture | four 0-byte files among them |
| The interface being in the picture; a camera move and a capture in one chunk catching an unpredictable frame | an earlier project of the maintainer's, which settled on moving in one call and capturing in the next |
| Behaviour paused, on a dedicated server, and in VR | §4: unmeasured |

---

## 2. Shape

### 2.1 One tool, one verb

| | |
|---|---|
| Tool | `dcs_screenshot`, arguments `host?`, `name?`, `wait_seconds?` |
| Verb | `dcs-mcp screenshot [--name N] [--wait-seconds S] [--out PATH]`, beside `status`, `ping`, `game-state` and `eval` |

Both go through the same function, as the five before them do, and both answer in the wording of
`mcp.md` §2.4: a word on the first line and a line each after it.

### 2.2 The chunk, and the host that may send it

The client publishes an ordinary `eval` request for the `hook` state, carrying a chunk it composes
itself:

```lua
DCS.makeScreenShot(<name>) return lfs.writedir()
```

The writedir comes back so the directory is the executor's own answer rather than the client's
arithmetic over `--saved-games` and `--variant`. Nothing else is read, and nothing is written from
inside DCS.

The capture runs in the `hook` state, so the hook host is the only host that can serve it. Asked of
the export host, which serves the `export` state alone, the tool answers `unsupported` and says
which host to ask. A DCS build where `DCS.makeScreenShot` is missing is an ordinary chunk error and
is reported as one, in the executor's own words.

### 2.3 The name

A name is a caller's to give, and the tool's to supply when they do not. The tool never sends an
empty name: an empty name means `Screen_YYMMDD_HHMMSS`, which is one-second granular and knowable
only by listing the directory afterwards and guessing which file is the answer, which is the work
this tool exists to remove.

**A caller's name** is one to sixty-four characters of `A-Z`, `a-z`, `0-9`, `_` and `-`. Anything
else is refused as `bad-request`, naming the character. The refusal is not caution: a dot loses the
picture (§1.3), a separator sends the file somewhere this has not measured, and a name is refused
rather than quietly rewritten so that the path in the answer is the path that was asked for.

**A name the tool supplies** is `dcs-eval-YYYYMMDD-HHMMSS-mmm` in local time, which sorts, says
where it came from, and does not collide at the rate a person or an agent can ask.

**A name already on disk** is not this document's business. DCS does whatever DCS does, and §4
leaves what that is unmeasured. The tool has no flag for it, refuses nothing, and deletes nothing.

### 2.4 When the capture is finished

Three things must hold before the tool answers `ok`, and all three are about telling this capture
from everything else in a directory that may hold hundreds of files:

1. **The file is there.** `<writedir>\ScreenShots\<name>` with `.png`, `.jpg`, `.bmp` or no
   extension at all — whichever arrives, since the format is the user's, no call reports it, and an
   abandoned capture carries the bare name (§1.3). Where more than one of those is newer than the
   request, the newest wins; where two share an instant, they are taken in the order `.png`,
   `.jpg`, `.bmp`, bare. The directory is not created: if DCS has never written a capture on this
   install there is nothing to watch, and the answer is `not-written` like any other absence.
2. **It is newer than the request.** Its modified time is at or after the clock reading taken
   *before* the request was published, so a file written while the request was still being
   published still counts as this capture's. Without this the tool would hand back a file of the
   same name from last week and call it the capture. This is how the tool stays right whatever DCS
   does about a name already taken: if DCS overwrites, the file is new and the answer is `ok`; if
   DCS refuses or writes elsewhere, nothing new appears and the answer is `not-written`, never the
   stale picture.
3. **It is whole.** The file ends the way its format ends: a PNG with its `IEND` chunk, a JPEG with
   `FF D9`, a BMP whose header's own size field matches the file. A size that has stopped growing
   is a guess about scheduling; the terminator is the file saying so.

A file that is there but not yet whole is looked at again: the directory is read every 100 ms
until one of the three holds or the wait runs out. A capture is zero bytes for part of its own
writing, so zero bytes is not an answer until the wait ends — a file that is still empty then is
answered `empty` rather than `not-written`, because something arrived and DCS abandoned it.

The same read gives the format and, from the header, the width and height — PNG's image header,
BMP's information header, and for JPEG the frame header, found by walking the markers so that a
thumbnail embedded in the metadata is not mistaken for the picture. They are in the answer because
a caller reasoning about anything on screen needs the resolution, and the file is the only place it
is written down.

### 2.5 What the answer says

| Word | When | Carries |
|---|---|---|
| `ok` | the three conditions of §2.4 held | `path`, `format`, `bytes`, `width`, `height` |
| `pending` | the executor did not answer within the wait | the id to collect under and the phase, as `mcp.md` §2.4 has it, plus the directory and the name the capture would take. Not an error |
| `not-written` | the executor answered, and no whole file newer than the request arrived before the wait ran out | the directory and the name it watched. Not an error: the file may still land, and on a machine that renders nothing it never will |
| `empty` | a file arrived under the name and was still zero bytes when the wait ended | the path, extension or none. It is what an abandoned capture leaves (§1.3), and the reason §2.4 watches the bare name as well as the three extensions |
| `bad-request` | the name broke §2.3 | the character that broke it |
| `unsupported` | asked of a host that cannot reach the capture | which host to ask |

Every other word is the executor's own, passed through unchanged: `no-session`, `stale-session`,
`run` for a chunk that raised, `bridge` for the executor's own failure, and the rest of `mcp.md`
§2.4's vocabulary. A refusal is marked an error and never reads as a call that came back empty.
`pending` and `not-written` are the two that are not refusals, and neither is marked one.

`dcs_collect` on a screenshot's id does what it does for any other reply: one pass, no waiting, and
it hands back what the executor answered — the write directory, not the picture. The file wait, the
wholeness test and the header read belong to this tool's own call and are not resumed elsewhere.
That is why a `pending` and a `not-written` carry the directory and the name rather than a path:
until a file exists, its extension is the user's setting and nobody here has read it (§2.4), so a
path would be a guess. A caller who collects has somewhere to look and one name to look for.

### 2.6 One wait, two phases

`wait_seconds` is one wait over both phases: the executor's reply, then the file. It defaults to the
15 seconds `mcp.md` §2.3 gives every other tool, and it is a wait and never a limit. A reply that
does not come inside it is `pending`, exactly as an `eval` would be. A reply that comes with the
file still missing spends what is left of the wait watching for it, and answers `not-written` when
it runs out.

Nothing here polls the executor. The request is published once, the wait is the client's, and the
file is watched from outside DCS where watching costs the game nothing.

### 2.7 The command line

`--name` and `--wait-seconds` carry the tool's arguments. `--out PATH` copies the finished file to
`PATH`, written wherever the caller points (ADR 0037).

This is a departure worth naming. `mcp.md` §2.1 has "the CLI verbs are the same functions with
`--out` writing the reply verbatim", and its §7 carries that as a control: "`--out` and `--capture`
write nothing for a `pending` and the reply verbatim otherwise". Here the reply is one line
saying where the write directory is, which nobody wants a copy of, and the thing the call went and
got is the picture. So on this verb `--out` writes the picture, and writes nothing at all unless
the answer is `ok`: a `pending`, a `not-written`, an `empty` or any refusal leaves `PATH` untouched
and says so, rather than leaving a file that looks like a capture.

`--capture` is not offered: it writes a reply file, and the reply here is one line of Lua's doing.

Exit codes are the binary's: 0 for an answer, including `pending` and `not-written`, 1 for a
refusal or a failed write, 2 for a command line that will not parse.

---

## 3. Out of scope, and why

- **Cropping.** One session in twenty-three that used screenshots cropped, and it needed window
  bounds read out of DCS to know where to cut, which this tool cannot know. A caller holding the
  path can crop with anything. The file DCS wrote stays the unmodified record.
- **Returning the picture in the answer.** An image block would let a client that cannot read the
  local file see the capture, and it costs an image decoder, a scaling pass and a judgement about
  quality. Every caller on record so far reads the file itself. It is left out until one cannot.
- **Aiming, pausing, hiding the interface.** All reachable through `eval`, and all needing their
  own call anyway: a camera move and a capture in one chunk catch an unpredictable frame.
- **Choosing the format.** `DCS.setScreenShotExt` has no read-back, so setting it means either
  leaving the user's choice changed or guessing what to restore. The tool reports the format that
  arrived instead.
- **Tidying.** No deletion, no renaming, no retention. Old captures are the user's files.
- **`net.screenshot_request`.** A server photographing its clients is a different capability with
  its own consent settings on both ends (§1.1).

---

## 4. What could not be determined

Each of these needs DCS running, and each is a decision record when it is answered.

| Question | What it changes |
|---|---|
| Does DCS overwrite a name already on disk, refuse it, or write elsewhere? | Nothing in the design: §2.4's newness test covers all three. It changes what the answer will be, and what the README should tell a user to expect |
| A capture asked for while the game is paused | Whether `not-written` is the normal answer while paused. Rendering continues, so it is expected to work |
| On a dedicated server | Expected to have no renderer and so no file, answering `not-written` after the full wait. If a file does appear, what it holds |
| In VR | Which eye, and how distorted. The community's account is the left eye, wide and distorted at the edges |
| A separator in the name | Whether a subdirectory is written, refused, or lost like a dot. §2.3 refuses one either way |
| `ScreenshotQuality` | Whether it reaches a JPEG capture taken this way, and whether it touches PNG at all |
| How long the file really takes | The two figures here are 86 ms apart in one burst and about a second after the call. A spread measured over many captures would say what a sensible wait is |

---

## 5. Controls the project must carry

Each is a check that must go red under a stated mutation, and each one's entry belongs in
`docs/mutations.md` in the pull request that builds it.

| Control | Reddens when |
|---|---|
| The name rules | a dot, a separator, an empty name or a sixty-fifth character is accepted |
| The supplied name's shape | two names taken one after the other collide |
| Completeness, per format | a truncated PNG, JPEG or BMP fixture is called whole |
| `empty` | a file that stays at zero bytes is waited out, or answered `ok` |
| The newness test | a complete file older than the request is answered as `ok` |
| The dimensions | a JPEG whose metadata carries a thumbnail reports the thumbnail's size |
| The host rule | the export host is served instead of refused |
| One wait over two phases | a file that arrives late in the wait is answered `not-written` |
| `pending` against `not-written` | a missing reply is answered `not-written`, or a missing file `pending` |
| Neither marked an error | `pending` or `not-written` comes back marked one |
| Seven tools listed | a tool registers without being listed |
| `--out` copies the file | the copy differs by a byte from the file DCS wrote |
| `--out` writes nothing else | a copy is written for an answer that is not `ok` |
| The exit codes | `not-written` exits 1 |

---

## 6. What this does not touch

The wire, the executor, the two ops, the request and reply format, the session layout, the
installer, the dormant budget, and the twelve game-state reads. If building this changes any of
them, something has been misread here and the change needs its own record.
