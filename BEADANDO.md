# Gaussian Blur — Multithreading Rust nyelven

**Tantárgy:** Párhuzamos programozás eszközei
**Téma:** Gaussian Blur képfeldolgozó algoritmus egy- és többszálú Rust
implementációja, teljesítménymérés és -elemzés

> A kódhoz tartozó, soronkénti/konstrukciónkénti Rust-nyelvi magyarázatok a
> [`docs/`](docs/) mappában találhatók (`docs/README.md` az index). Ez a
> dokumentum a beadandó formális jelentése: a *miért* és a *mit mértem*
> kérdésekre fókuszál, a *hogyan működik pontosan a kód soronként* kérdésre a
> `docs/*.md` fájlok adnak választ.

---

## 1. Feladat célja

A feladat egy Gaussian Blur (Gauss-elmosás) képfeldolgozó szűrő megvalósítása
Rust nyelven, **elsősorban a többszálú (multithreading) programozás
bemutatása céljából** — a Gaussian Blur maga csak egy jól párhuzamosítható,
környezetfüggő (neighborhood-based) algoritmus, ami alkalmas arra, hogy
rajta keresztül konkrétan, mérhetően bemutassam:

- hogyan lehet egy számításigényes algoritmust biztonságosan több CPU-szálra
  osztani Rustban,
- hogyan garantálja a Rust típusrendszere (borrow checker) az adatverseny-
  mentességet MÁR FORDÍTÁSI IDŐBEN,
- hogyan mérhető és értékelhető objektíven a párhuzamosítás nyeresége
  (speedup, hatékonyság),
- milyen szerepe van a CPU cache-nek a párhuzamos teljesítményben.

A cél NEM egy ipari minőségű, minden formátumot/optimalizációt támogató
képfeldolgozó könyvtár megírása, hanem egy jól követhető, minden lépésében
ellenőrzött, egyetemi szintű demonstráció.

## 2. Elméleti háttér

**Kapcsolódás a tananyaghoz:** a tantárgy "05 Környezetfüggő
képfeldolgozás" témakörének hivatalos hivatkozáslistája kifejezetten
megnevezi a Gaussian Blur-t (`https://en.wikipedia.org/wiki/Gaussian_blur`),
az élkeresés (Edge detection), az unsharp masking és a medián szűrő
mellett — ez a projekt tehát közvetlenül ehhez a témakörhöz, valamint a
"02 Multithreading" témakörhöz kapcsolódik.

**Kép reprezentáció:** a bemenet RGB8 formátumú PNG kép — minden pixel 3
csatornán (piros, zöld, kék), csatornánként `u8` (0-255 közötti, 8 biten
tárolt) intenzitásértékkel. A képet az `image` crate tölti be/menti, a
pixeleket egy sorfolytonos (row-major) `Vec<u8>`-ban tárolva.

**Konvolúció és kernel:** a Gaussian Blur egy KÖRNYEZETFÜGGŐ szűrő — egy
pixel új értéke a körülötte lévő pixelek SÚLYOZOTT ÁTLAGA. A súlyokat a
**kernel** (konvolúciós mag) adja meg; nálam ez egy `[1,2,1;2,4,2;1,2,1]`
3×3-as (később ált. `2r+1 × 2r+1` méretű) mátrix, ami egy diszkrét
közelítése a folytonos Gauss-görbének.

**Szeparábilis konvolúció:** a Gauss-kernel felbontható egy oszlop- és egy
sorvektor szorzatára (`[1,2,1] ⊗ [1,2,1]`), ezért a 2D konvolúció
helyettesíthető két egymás utáni 1D konvolúcióval (vízszintes, majd
függőleges) — ez matematikailag PONTOSAN ugyanazt az eredményt adja, de
kevesebb szorzást igényel (`2k` a `k²` helyett egy `k`-elemű kernelnél).

**Cache-hierarchia:** a CPU L1 (leggyorsabb, ~32-64 KB/mag), L2 (~256 KB - 1
MB/mag) és L3 (több MB, megosztott) cache-eket használ a RAM-nál gyorsabb
adathozzáféréshez, 64 bájtos "cache line" egységekben. A memóriához való
SORFOLYTONOS (szekvenciális) hozzáférés jóval kevesebb "cache miss"-t okoz,
mint az "ugráló" hozzáférés — ez indokolja a csempézett (tiled) algoritmust.
Részletek: [`docs/blur.md`](docs/blur.md).

## 3. Algoritmus

A projektben HÁROM, egymásra épülő, de FUNKCIONÁLISAN EGYENÉRTÉKŰ (mindegyik
pixelre pontosan ugyanazt az eredményt adó) implementáció készült:

1. **Direkt 2D konvolúció** — a legegyszerűbb, "tankönyvi" referencia.
2. **Szeparábilis (két 1D lépéses) konvolúció** — kevesebb szorzás.
3. **Csempézett (tiled), kígyózó (boustrophedon) bejárású, cache-
   optimalizált konvolúció** — az L1 cache-hez igazított munkaterület,
   ez a végleges algoritmus, amit párhuzamosítottam.

Mindhárom "clamp-to-edge" szélkezelést használ: a kép szélén a hiányzó
szomszédokat a legközelebbi érvényes pixellel helyettesítem (ahelyett hogy
feketével/nullával töltenék fel, vagy körbe csavarnék a képet).

## 4. Egyszálú implementáció

A **direkt 2D konvolúció** (`blur::gaussian_blur_3x3`) minden pixelre
kiszámolja a 3×3 szomszédság súlyozott átlagát, egy VADONATÚJ kimeneti
képbe írva (soha nem olvas vissza a saját, már kiszámolt eredményéből — l.
6. pont). Ez a REFERENCIA, amihez minden további implementációt
hasonlítok.

A **szeparábilis** változat (`horizontal_pass` + `vertical_pass`) egy
lebegőpontos (`f32`) köztes képet használ a két lépés között, hogy ne
veszítsek pontosságot kerekítéssel — ez bit-pontosan ugyanazt az eredményt
adja, mint a direkt módszer (ellenőrizve, 0 pixel eltérés).

A **csempézett** változat 16×16 pixeles csempékre bontja a képet, mindegyiket
függetlenül (a szükséges +1 pixeles "halo" perem újraszámolásával)
dolgozza fel, **kígyózó** (soronként váltakozó irányú) sorrendben — ez adja
a végleges, párhuzamosításra kész algoritmust. Részletek: [`docs/blur.md`](docs/blur.md).

**Mért egyszálú idők** (2048×2048, egyszeri mérés, tájékoztató jelleggel — a
hivatalos, statisztikailag megalapozott mérés a 8-9. pontban):

| Implementáció | Idő | Eltérés a referenciától |
|---|---|---|
| Direkt (naiv 2D) | ~74-82 ms | — (ez maga a referencia) |
| Szeparábilis | ~51-60 ms | 0 pixel |
| Csempézett + kígyózó | ~71-75 ms | 0 pixel |

A csempézett verzió egyszálon NEM gyorsabb a szeparábilisnál — ennek okait
lásd a 12. és 13. pontban.

## 5. Többszálú implementáció

A `blur::gaussian_blur_3x3_tiled_mt(input, num_threads)` a csempézett
algoritmust futtatja `num_threads` szálon, **`std::thread::scope`**
segítségével:

```rust
std::thread::scope(|scope| {
    for (chunk_index, chunk) in raw.chunks_mut(chunk_bytes).enumerate() {
        scope.spawn(move || {
            process_tile_row_range(input, chunk, ...);
        });
    }
});
```

A `thread::scope` GARANTÁLJA, hogy minden elindított szál befejeződik,
mielőtt a `scope` hívás visszatér — ez teszi lehetővé, hogy a szálak
BIZTONSÁGOSAN kölcsönözzenek nem-`'static` adatot (a bemeneti képet), `Arc`
nélkül. Minden szál a bemeneti képet csak OLVASSA (biztonságos több szálról
is), a kimenetnek pedig egy DISZJUNKT (nem átfedő) darabját kapja írásra.
Részletek (`chunks_mut`, `scope.spawn`, `move`, `Send`): [`docs/blur.md`](docs/blur.md).

Összehasonlításképpen elkészült egy **Arc/Mutex-alapú** változat is
(`gaussian_blur_3x3_tiled_mutex`), ami ugyanazt az eredményt adja, de minden
csempe után zárolja a teljes kimeneti képet — ennek teljesítmény-hatását a
9. pont mutatja. Részletek: [`docs/mutex_demo.md`](docs/mutex_demo.md).

## 6. Adatfelosztás

A kép feldolgozását **ÖSSZEFÜGGŐ, csempesor-igazított sávokra (row-block)**
osztottam szét a szálak között — NEM minden n-edik sort/pixelt külön
szálnak. Ennek két oka van:

1. **Cache locality**: egy összefüggő sávban a memóriaelérés szekvenciális,
   ami kihasználja a CPU előretöltését (prefetch) és a térbeli lokalitást.
2. **False sharing elkerülése**: ha egymás melletti pixeleket/sorokat
   különböző szálak írnának, a közös cache line-ok folyamatosan
   "ugrálnának" a magok között (cache-koherencia overhead) — ez tisztán
   teljesítményprobléma, nem helyességi, de jelentősen ronthatja a
   skálázódást. Az összefüggő, nagy (legalább `TILE_SIZE`=16 sornyi) sávok
   ezt gyakorlatilag kiküszöbölik (a sávhatárok elenyésző hányadát teszik
   ki a teljes munkának). Részletek: [`docs/mutex_demo.md`](docs/mutex_demo.md).

A diszjunkt felosztást a `[u8]::chunks_mut()` standard könyvtári metódus
végzi, ami FORDÍTÁSI IDŐBEN garantálja a nem-átfedést (l. 7. pont). Minden
csempe a saját "halo" (ghost cell) peremét önállóan, redundánsan újraszámolja,
hogy a csempék (és így a szálak munkaterületei) teljesen FÜGGETLENEK
legyenek egymástól — nincs szükség szálak közti kommunikációra vagy
szinkronizációra a feldolgozás közben.

## 7. Rust memóriabiztonság

A borrow checker (kölcsönzés-ellenőrző) a fordítási idő alatt garantálja:

- Egy adatra egyszerre csak EGY `&mut` (módosítható) referencia létezhet, VAGY
  tetszőleges számú `&` (csak olvasható) referencia — sosem mindkettő
  egyszerre. Emiatt a bemeneti kép (`&RgbImage`, csak olvasott) biztonságosan
  megosztható minden szál között, míg a kimenet `&mut [u8]` szeleteit a
  `chunks_mut` garantáltan DISZJUNKTAN adja ki — a fordító ebből
  BIZONYÍTJA, hogy nem lehet két szál között adatverseny (data race), anélkül
  hogy futásidőben bármit ellenőrizni kellene.
- A `std::thread::scope` élettartam-rendszere (lifetime) biztosítja, hogy a
  szálak nem élhetnek túl semmilyen kölcsönzött adatot — a `scope` hívás
  csak azután térhet vissza, hogy minden szál lefutott.

Ennek eredményeként a végleges (`gaussian_blur_3x3_tiled_mt`) implementáció
**egyáltalán nem használ zárolást (`Mutex`) vagy megosztott tulajdonlást
(`Arc`)** — nincs is rá szükség, mert nincs megosztott, egyszerre írható
állapot. Ezt a döntést egy KÜLÖN, oktatási célú `Arc<Mutex<...>>`-alapú
változattal (4. pont, `gaussian_blur_3x3_tiled_mutex`) hasonlítottam össze,
lásd 9. pont. Részletek: [`docs/blur.md`](docs/blur.md), [`docs/mutex_demo.md`](docs/mutex_demo.md).

## 8. Benchmark módszertan

- Az időmérés (`std::time::Instant`) **KIZÁRÓLAG** a Gaussian Blur SZÁMÍTÁS
  körül fut — a PNG beolvasása/dekódolása és a kimenet lemezre mentése
  MINDIG a mérésen KÍVÜL történik, hogy a mérés valóban csak az algoritmus
  sebességét tükrözze.
- Minden szálszámot **15-ször** megismételtem (`benchmark::measure`), és
  kiszámoltam a minimumot, maximumot, átlagot és mediánt. A medián a
  "tipikus" viselkedést jobban közelíti, mint az átlag, mert nem
  érzékeny egy-egy kiugró (pl. háttérfolyamat által megzavart) mérésre.
- `std::hint::black_box` védi a mérést attól, hogy az optimalizáló fordító
  "átlásson" rajta és kihagyja a számítást.
- Minden optimalizált/párhuzamosított változatot **programozottan**
  (pixelenkénti összehasonlítással) ellenőriztem a direkt referenciával
  szemben, MINDEN mérés előtt — soha nem fogadtam el egy gyorsabb, de más
  eredményt adó implementációt. Részletek: [`docs/benchmark.md`](docs/benchmark.md), [`docs/main.md`](docs/main.md).

## 9. Mérési eredmények

**Szálszám-sweep** (2048×2048, 3×3 kernel, csempézett algoritmus, 16 logikai
mag, 15 ismétlés/szálszám):

| Szálak | Min (ms) | Max (ms) | Átlag (ms) | Medián (ms) | Eltérés a referenciától |
|---|---|---|---|---|---|
| 1 | 70.3 | 79.9 | 74.5 | 73.8 | 0 pixel |
| 2 | 35.7 | 39.3 | 37.3 | 37.2 | 0 pixel |
| 4 | 19.1 | 23.7 | 21.0 | 20.3 | 0 pixel |
| 8 | 11.4 | 19.4 | 16.6 | 16.5 | 0 pixel |

**Lock-free vs. Arc/Mutex összehasonlítás** (ugyanaz a kép/kernel, 10
ismétlés):

| Szálak | Lock-free medián | Mutex medián | Lassulás |
|---|---|---|---|
| 1 | 74.7 ms | 71.2 ms | 0.95x |
| 2 | 38.4 ms | 38.0 ms | 0.99x |
| 4 | 21.4 ms | 36.5 ms | 1.71x |
| 8 | 18.3 ms | 29.2 ms | 1.59x |

**Méret- és kernelméret-sweep** (`src/bin/size_kernel_sweep.rs`): 5
képméret (512-8192) × 4 kernelméret (3×3-9×9) × 4 szálszám, 5 ismétlés/
kombináció, medián alapján. A teljes, 80 soros nyers táblázat:
[`docs/sweep_results.md`](docs/sweep_results.md). MINDEN kombináció **0
pixel eltéréssel** egyezett a függetlenül számolt direkt 2D referenciával.

Egyszálú futási idők (medián, ms) — jól látszik a méret- (kb. négyzetesen)
és kernelméret-arányos (kb. lineárisan, a szeparábilis felbontás miatt)
növekedés:

| Méret \\ Kernel | 3×3 | 5×5 | 7×7 | 9×9 |
|---|---|---|---|---|
| 512 | 5.3 | 7.0 | 9.6 | 12.7 |
| 1024 | 20.5 | 28.7 | 39.4 | 51.2 |
| 2048 | 79.2 | 117.6 | 154.5 | 197.6 |
| 4096 | 314.5 | 448.7 | 599.8 | 772.4 |
| 8192 | 1234.0 | 1803.1 | 2618.4 | 3141.4 |

Speedup 8 szálon (`S(8) = T₁/T₈`) minden (méret, kernel) kombinációra:

| Méret \\ Kernel | 3×3 | 5×5 | 7×7 | 9×9 |
|---|---|---|---|---|
| 512 | 3.71x | 3.29x | 5.09x | 5.77x |
| 1024 | 4.13x | 4.03x | 4.35x | 4.37x |
| 2048 | 4.33x | 4.71x | 4.51x | 4.79x |
| 4096 | 5.06x | 5.16x | 5.05x | 4.98x |
| 8192 | 4.79x | 5.24x | 5.68x | 5.43x |

Hatékonyság 8 szálon (`E(8) = S(8)/8`) minden kombinációra:

| Méret \\ Kernel | 3×3 | 5×5 | 7×7 | 9×9 |
|---|---|---|---|---|
| 512 | 46.4% | 41.1% | 63.7% | 72.2% |
| 1024 | 51.6% | 50.3% | 54.4% | 54.6% |
| 2048 | 54.1% | 58.9% | 56.4% | 59.9% |
| 4096 | 63.2% | 64.5% | 63.1% | 62.3% |
| 8192 | 59.8% | 65.4% | 71.0% | 67.8% |

**Explicit SIMD kiegészítés** (`accumulate_channels`, SSE2 intrinsic-ek a
`process_tile` csatorna-összegzésében — l. [`docs/blur.md`](docs/blur.md)):
a fenti szálszám-sweep (2048×2048, 3×3, csempézett) SIMD UTÁNI, 3 futás
átlagolt mediánja, a SIMD ELŐTTI (fenti táblázat) értékekhez képest:

| Szálak | SIMD előtt | SIMD után | Változás |
|---|---|---|---|
| 1 | 73.8 ms | ~69.6 ms | ~6% gyorsabb |
| 2 | 37.2 ms | ~36.5 ms | ~2% gyorsabb (zaj szintje) |
| 4 | 20.3 ms | ~22.6 ms | ~11% **lassabb** (konzisztensen, 3/3 futásnál) |
| 8 | 16.5 ms | ~14.6 ms | ~11% gyorsabb (konzisztensen) |

Minden SIMD utáni mérés **0 pixel eltéréssel** egyezett a referenciával.

## 10. Speedup

A speedup képlete: **S(p) = T₁ / Tₚ**, ahol T₁ az egyszálú, Tₚ a p-szálas
medián futási idő.

| Szálak (p) | S(p) |
|---|---|
| 1 | 1.00x |
| 2 | 1.99x |
| 4 | 3.64x |
| 8 | 4.47x |

A speedup 4 szálig gyakorlatilag LINEÁRIS (közel ideális), 8 szálnál
lelassul a növekedés üteme. A teljes méret-/kernelméret-sweep (9. pont)
alapján ez a minta ÁLTALÁNOSAN igaz marad: a 8 szálas speedup minden
vizsgált (méret, kernel) kombinációnál 3.3x-5.8x között alakul — a
legnagyobb (8192, 9×9 és 7×7) kombinációknál a legjobb (5.4-5.7x).

## 11. Hatékonyság

A hatékonyság képlete: **E(p) = S(p) / p**, azaz mekkora hányadát
"hasznosítom" ténylegesen a hozzáadott szálaknak.

| Szálak (p) | E(p) |
|---|---|
| 1 | 100% |
| 2 | 99% |
| 4 | 91% |
| 8 | 56% |

4 szálig szinte tökéletes (~90-100%) a hatékonyság, 8 szálnál 56%-ra esik
vissza — ennek okait a 12-13. pont tárgyalja. A méret-/kernelméret-sweep
(9. pont) alapján a hatékonyság 8 szálon ÁLTALÁBAN NŐ a képmérettel (512-nél
41-72%, 4096-nál 62-65%) — ez arra utal, hogy a szálindítási és
csempézési overhead ARÁNYOSAN kisebb súllyal esik latba, minél nagyobb a
tényleges hasznos munka szálanként.

## 12. Cache hatása

- A **szeparábilis** algoritmus vízszintes lépése cache-BARÁT (a memóriában
  egymás melletti bájtokat olvas), a függőleges lépése viszont
  cache-ELLENSÉGES (soronként `szélesség × 3 × 4` bájtot ugrik, egy 2048
  széles képnél 24 576 bájtot — jóval többet, mint egy 64 bájtos cache line).
- A **csempézés** ezt orvosolja: egy 16×16-os csempe köztes puffere
  (~3.5 KB) kényelmesen elfér az L1 cache-ben (tipikusan 32-64 KB), így a
  teljes csempe feldolgozása alatt nem kell L1-en túlra (L2/L3/RAM) nyúlni
  ugyanazért az adatért kétszer.
- A **kígyózó (boustrophedon) bejárás** biztosítja, hogy két egymás után
  feldolgozott csempe mindig szomszédos legyen a képen (nem kell a kép
  másik szélére ugrani sorváltáskor) — ez növeli az esélyét, hogy a
  csempehatáron megosztott adatok még "melegen" (cache-ben) legyenek.
- **Egyszálon** a csempézés rezsije (redundáns halo-számítás + csempénkénti
  `Vec`-allokáció, 2048×2048-nál 16384 csempe) MEGHALADJA a cache-előnyt egy
  ilyen kicsi (3×3-as) kernelnél — ez látszik a 4. pont mérésén (72-75 ms a
  szeparábilis 51-60 ms-jával szemben). **Több szálon** viszont a csempézés
  válik a párhuzamosítás természetes alapegységévé (l. 5-6. pont), és a
  teljes rendszer együttesen (csempézés + szálak) egyértelműen kifizetődik
  (4.47x speedup 8 szálon).
- A **false sharing** elkerülése (6. pont) szintén cache-koherencia
  kérdés — a nagy, összefüggő szálankénti sávok ezt gyakorlatilag
  megszüntetik.

## 13. Eredmények értékelése

A mérések összhangban vannak az elmélettel:

- **4 szálig közel lineáris speedup** (91-99% hatékonyság) — a 16384
  csempe könnyen, kiegyensúlyozottan osztható szét, a szálak közötti
  kommunikáció/szinkronizáció (nulla, hiszen nincs megosztott írható
  állapot) nem jelent overhead-et.
- **8 szálnál a hatékonyság 56%-ra esik** — ennek valószínű okai: a
  feladat (2048×2048, 3×3 kernel) egyszálon már csak ~74-82 ms, ezen a
  méreten a szálindítási/leállítási overhead és a memória-sávszélesség
  korlátai (több szál egyszerre kompetál ugyanazért a RAM/L3
  sávszélességért) arányaiban egyre nagyobb szeletet visznek el a
  teljes időből, ahogy a hasznos munka szálankénti része csökken.
- A **Mutex-es változat lock contention-je** jól látható és mérhető (4
  szálnál 1.71x lassulás a lock-free változathoz képest) — ez konkrét,
  számszerű bizonyíték arra, hogy a zárolás elkerülése (ahol lehetséges)
  valós teljesítménynyereséget jelent, nem csak elméleti ajánlás.
- A teljes méret-/kernelméret-sweep (9. pont) megerősíti és PONTOSÍTJA a
  2048×2048/3×3-as mérést: a hatékonyság 8 szálon ÁLTALÁBAN JAVUL a
  nagyobb képméretekkel (512-nél 41-72%, 4096-8192-nél 60-71%) — minél
  nagyobb a tényleges számítási munka szálanként, annál kisebb súllyal esik
  latba a fix overhead (szálindítás, csempénkénti allokáció). A legkisebb
  (512×512) képnél a 3×3/5×5 kernelek kifejezetten alacsony (41-46%)
  hatékonyságot mutatnak — itt a teljes munka (5-7 ms) összemérhető a
  szálkezelés rezsijével.
- A nagyobb kernelek (7×7, 9×9) ÁLTALÁBAN JOBB speedupot/hatékonyságot
  érnek el, mint a kisebbek (3×3, 5×5) ugyanazon a méreten — ez
  összhangban van azzal, hogy nagyobb kernelnél arányosan TÖBB számítás
  jut minden bájtnyi memóriaforgalomra (jobb "compute-to-memory" arány),
  ami kevésbé teszi memóriasávszélesség-korlátozottá (memory-bound) a
  feladatot.
- Az **explicit SSE2 SIMD kiegészítés** (9. pont) VEGYES eredményt adott:
  8 szálon konzisztensen ~11%-kal gyorsított, 1 szálon szerényebben
  (~6%), 4 szálon viszont KONZISZTENSEN ~11%-kal LASSÍTOTT. A korrektség
  mindvégig 0 pixel eltéréssel megmaradt. A szerény/vegyes eredmény
  valószínű oka, hogy a csatorna-szintű (csak 3 a 4 SIMD lane-ből
  hasznos) vektorizálás NEM összefüggő memória-betöltéssel dolgozik,
  hanem `_mm_set_ps`-sel "összerakott" skalár értékekkel — ennek rezsije
  jelentősen csökkenti az elméleti nyereséget. Ez konkrét, mért bizonyíték
  arra, hogy egy optimalizációs technika (itt: SIMD) bevezetése ÖNMAGÁBAN
  NEM garantál gyorsulást — a KONKRÉT adatelrendezéshez (itt: egymásba
  font R,G,B csatornák) illesztett implementáció szükséges a valódi
  kihasználáshoz.

## 14. Korlátok

- A csempézett algoritmus fix, `TILE_SIZE`=16 méretű csempéket használ —
  ez nem feltétlen optimális minden kernelméretre/CPU cache-méretre; egy
  hangolható csempeméret (pl. a tényleges L1 méretéhez igazítva) tovább
  javíthatná az eredményt, különösen nagyobb kernelméreteknél.
- A csempénkénti `Vec<f32>` allokáció (a köztes vízszintes eredményhez)
  minden csempe-hívásnál újra lefoglalódik — egy újrafelhasznált,
  szálankénti puffer csökkenthetné ezt a rezsit.
- A teljesítménymérés egyetlen konkrét gépen (16 logikai mag) történt —
  más CPU-architektúrákon (más cache-méret, más mag-/szálszám) az
  optimális csempeméret és a hatékonyság-görbe eltérhet.
- A méret-/kernelméret-sweep 5 ismétlést használ kombinációnként (a 400
  kombináció összesített futásideje miatt), szemben a fő szálszám-
  benchmark 15 ismétlésével — ez valamivel zajosabb medián-becslést ad
  (ez látszik is néhány nem teljesen monoton értéken, pl. az 512×512/5×5
  8 szálas 41.1%-os hatékonyságán, ami alacsonyabb, mint a 4 szálas 83.7%
  alapján várható lenne).
- Az explicit SIMD kiegészítés 4 szálon mért, konzisztens (~11%)
  LASSULÁSÁNAK pontos okát NEM vizsgáltam mélyebben (pl. generált gépi
  kód méretének/regiszter-nyomásának hatása az ütemezésre ezen a konkrét
  CPU-n) — ez egy nyitva hagyott, magyarázatra szoruló megfigyelés.
- A SIMD-implementáció csak x86-64 architektúrán aktív
  (`#[cfg(target_arch = "x86_64")]`); ARM/Apple Silicon gépeken a sima
  skalár fallback fut (funkcionálisan helyes, de nem gyorsított).

## 15. Továbbfejlesztési lehetőségek

- Hangolható `TILE_SIZE`, esetleg a tényleges CPU cache-méret futásidejű
  lekérdezése alapján.
- Csempénkénti puffer újrafelhasználása (allokáció-mentesítés).
- `Rayon` crate-tel készült, magas szintű párhuzamos változat mint
  HARMADIK, opcionális összehasonlítási alap a kézzel írt `std::thread`
  megoldás mellett.
- **Pixelenkénti (nem csatornánkénti) SIMD-vektorizálás**: a jelenlegi
  `accumulate_channels` csak 3 a 4 SIMD lane-ből használ ki, és nem
  összefüggő memória-betöltéssel dolgozik. Nagyobb, megbízhatóbb
  nyereséghez a R,G,B csatornákat elő kellene SZÉTVÁLASZTANI (de-
  interleaving, "Structure of Arrays" elrendezés), hogy egyszerre 4-8
  EGYMÁS MELLETTI pixel ugyanazon csatornáját lehessen egyetlen,
  ÖSSZEFÜGGŐ SIMD-betöltéssel feldolgozni — ez jelentősebb átalakítás
  lenne, de valószínűleg konzisztensebb, nagyobb speedupot adna minden
  szálszámnál.
- Memory-mapped (`MmapMut`) kép-tárolás nagyon nagy (a rendelkezésre álló
  RAM-hoz képest jelentős méretű) képekhez — megfontoltam, de a jelenlegi
  képméret-tartományban (max 8192×8192, ~200 MB) nem indokolt, mert
  gyakorlatban sosem futna le ez az ág egyetlen reális gépen sem (l.
  [`docs/blur.md`](docs/blur.md) "Továbbfejlesztési ötletek" szakasza).
