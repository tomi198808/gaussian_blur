# Gaussian Blur — többszálú képfeldolgozás Rustban

## A projekt röviden

Egy 2048×2048-as PNG képre 3×3-as Gauss-kernellel elmosást (Gaussian Blur) végzek, és azt vizsgálom, hogyan lehet ezt `std::thread`-del gyorsítani. Három egyszálú változatot írtam, ezekre épül a többszálú:

| Változat | Mit csinál |
|---|---|
| **direkt** | Minden pixelre végigmegy a 3×3 kernelen. Ez a referencia: ehhez hasonlítok mindent. |
| **szeparábilis** | A 2D kernelt két 1D lépésre bontom (előbb vízszintes, majd függőleges menet). |
| **csempézett + kígyózó** | A képet 16×16-os csempékre bontom, így az adatok a gyors L1 cache-ben maradnak. A csempék saját +1 pixeles szegélyt (halo) kapnak, ezért függetlenül számolhatók. A többszálú változat ezt osztja szét a szálak között. |

Mindhárom változat bit-pontosan ugyanazt az eredményt adja. A program ezt minden futáskor ellenőrzi. A csatornák összegzését egy helyen kézzel írt SSE2 SIMD gyorsítja (`accumulate_channels`).

A részletes magyarázatok a `docs/` mappában vannak, a formális jelentés a [`BEADANDO.md`](BEADANDO.md).

## Futtatás

```bash
cargo run --release --bin gen_test_image   # sakktábla teszt kép: images/input.png
cargo run --release                        # fő program + benchmark
cargo run --release --bin mutex_demo       # lock-free vs Mutex összehasonlítás
cargo run --release --bin size_kernel_sweep  # méret- és kernelméret-sweep (hosszú)
```

Mindig `--release`-szel futtatok. Debug módban az idők értelmetlenek.

## Mit ír ki a fő program?

Az alábbi kimenet egy valódi futásból származik (i7-11800H, 8 fizikai mag / 16 logikai). A számok gépenként és futásonként kicsit változnak.

```text
Kép betöltve: images/input.png
Szélesség:    2048 px
Magasság:     2048 px
Pixel formátum (ColorType): Rgb8
Gaussian Blur (3x3, 1 szál) futási ideje: 78 ms
Gaussian Blur (3x3, szeparabilis, 1 szal) futasi ideje: 85 ms
Direkt vs. szeparabilis: 0 pixel ter el egymastol (a celom: 0)
Gaussian Blur (3x3, csempezett+kigyozo, 1 szal) futasi ideje: 63 ms
Direkt vs. csempezett: 0 pixel ter el egymastol (a celom: 0)
Logikai CPU magok (available_parallelism): 16
```

- **Az első négy sor** a betöltött képet írja le. Az `Rgb8` azt jelenti, hogy pixelenként 3 csatorna van, csatornánként 8 bit.
- **Az egyszálú futási idők** egyetlen méréssel készültek (`Instant`), kerekítve ezredmásodpercre. Tájékoztató jellegűek, a pontos mérés lejjebb van. A direkt változat 78 ms, a csempézett 63 ms, tehát a cache-barát bejárás mérhetően gyorsabb. A szeparábilis változat 3×3-as kernelnél nem gyorsabb a direktnél (85 ms). A kernel kicsi, ezért nem térül meg a második menet és az `f32` köztes kép többletköltsége. A szeparábilis felbontás nagyobb kernelnél válik előnyössé.
- **„0 pixel tér el egymástól"** a korrektségi ellenőrzés: a pixelenkénti összehasonlítás 0 eltérést talált a direkt referenciához képest. Ha ez nem 0 lenne, a program leállna (`assert_eq!`). Optimalizálni csak úgy engedek, hogy a végeredmény nem változik.
- **A logikai magok száma** a gépről szól. A szálszámok 1, 2, 4, 8, ami a 16 logikai magon belül marad.

### A benchmark táblázat

```text
Benchmark: 15 ismétlés szálszámonként (2048x2048, 3x3 kernel, csempézett algoritmus)
szálak |   min ms |   max ms | átlag ms | medián ms |  speedup | haték.
     1 |    62.62 |    71.90 |    65.15 |    64.81 |    1.00x | 100.0%
     2 |    31.71 |    46.71 |    35.17 |    33.69 |    1.92x |  96.2%
     4 |    17.95 |    28.85 |    22.01 |    21.21 |    3.06x |  76.4%
     8 |    11.59 |    15.45 |    13.95 |    13.82 |    4.69x |  58.6%
```

Minden szálszámot 15-ször mérek meg, és a 15 futásból számolom az oszlopokat:

| Oszlop | Jelentése |
|---|---|
| **szálak** | Ennyi szálon fut a csempézett algoritmus. |
| **min / max** | A leggyorsabb és a leglassabb futás a 15-ből. A kettő távolsága mutatja a zajt. |
| **átlag** | A 15 futás számtani közepe. Egy-egy kiugró lassú futás felhúzza. |
| **medián** | A rendezett idők közepe. Ez a legmegbízhatóbb szám, a speedup is ebből készül. |
| **speedup** | S(p) = T₁ / Tₚ: hányszor gyorsabb a p szálas futás az 1 szálasnál (mediánokból). |
| **haték.** | E(p) = S(p) / p: a szálak mekkora részét hasznosítom ténylegesen. 100% a tökéletes, ideális skálázás. |

**Mit látok belőle:**

- 2 szálon a gyorsulás 1.92×, a hatékonyság 96%, tehát szinte ideális a skálázás.
- 4 szálon 3.06× (76%), 8 szálon 4.69× (59%). A szálak számával a hatékonyság esik, és ez normális. Az okok: a szálindítás és az összevárás fix költsége, a memóriasávszélesség megosztása, és hogy a 8 szál egy része akár egy fizikai mag két hiperszálán is futhat. Ezeket az okokat nem mértem külön, a magyarázat feltételezés.
- A medián és a max közötti különbség (pl. 2 szálnál 33.7 és 46.7 ms) mutatja, hogy egyetlen futás nem elég, ezért ismétlem a mérést.
- A mérés csak a számítást időzíti. A fájlbeolvasás és a mentés kívül van, a korrektségi ellenőrzés pedig külön, nem mért futás.

### Az ellenőrző sor és a kivágások

```text
Ellenorzes: 121980 / 4194304 pixel valtozott, legnagyobb csatorna-elteres: 96
```

Ez azt bizonyítja, hogy az elmosás ténylegesen csinált valamit. A 4 194 304 pixelből csak 121 980 változott. A sakktábla belsejében a szomszédok egyformák, tehát nincs mit elmosni. A változás csak a négyzetek határán van. A legnagyobb csatornánkénti eltérés 96 (a 0–255 skálán). A program elmenti a `images/crop_before.png` és `images/crop_after.png` fájlokat is: egy sakktábla-határ 8-szor nagyítva, elmosás előtt és után.

## `mutex_demo` — miért nem használok zárolást?

```text
szálak |  lock-free |      mutex |   lassulás
     1 |    64.71 ms |    67.93 ms |      1.05x
     2 |    34.17 ms |    39.38 ms |      1.15x
     4 |    17.81 ms |    23.49 ms |      1.32x
     8 |    14.77 ms |    24.15 ms |      1.63x
```

Ugyanazt a számítást kétféleképpen futtatja: **lock-free** (a kimeneti kép diszjunkt darabjait kapja minden szál, nincs zár) és **mutex** (egy `Arc<Mutex<…>>` védi a közös kimenetet, minden csempe után zárolni kell). A **lassulás** oszlop a mutex és a lock-free medián hányadosa. Mindkét változat 0 pixel eltérést ad a referenciához képest. A zárolás egy szálnál szinte semmibe sem kerül (1.05×), de a szálak számával nő az egymásra várakozás (lock contention), 8 szálnál már 1.63× lassabb. Ezért választottam a diszjunkt darabokra bontást.

## `size_kernel_sweep` — képméret és kernelméret hatása

A program 512×512-től 8192×8192-ig, 3×3-tól 9×9-es kernelig, 1/2/4/8 szálon méri a csempézett algoritmust. Soronként ezt írja ki: kernel, szálak, medián (ms), speedup, hatékonyság. Az eredményt a [`docs/sweep_results.md`](docs/sweep_results.md) táblázataiba is elmenti. Először kernelméretenként egy kis képen ellenőrzi, hogy az eredmény 0 pixellel egyezik a direkt referenciával.

Amit a táblázatokból látok: kis képen és kis kernelnél kevesebb a hasznos munka szálanként, ezért a szálindítás rezsije nagyobb arányú és a hatékonyság rosszabb. Nagyobb kernelnél (7×7, 9×9) több a munka pixelenként, ezért 8 szálon is jobb a skálázás (a 512×512-es képen a 9×9-es kernel 5.77× gyorsulást ad 8 szálon, a 3×3-as csak 3.71×-t).

## Mit tanultam a mérésekből?

- A csempézett, cache-barát bejárás már egy szálon is gyorsabb a naiv változatnál.
- A többszálú gyorsulás valódi, de nem ideális: 8 szálon kb. 4.7×.
- A zárolás mérhetően lassít, a diszjunkt darabokra bontás zár nélkül működik.
- A SIMD-nyereség vegyes: 8 szálon kb. 11%-kal gyorsabb, 1 szálon kb. 6%-kal, 4 szálon viszont egy korábbi mérésben kb. 11%-kal lassabb volt. Ennek az okát nem sikerült tisztázni, ezt a jelentésben is leírtam.
