# Test album — licenses and attribution

Stand-in test album for the GBA album project (codec test material). It mirrors the track count and approximate track
durations of Queen's *A Night at the Opera* (12 tracks, ~42:52), but contains **no Queen material**. Every track below is
freely licensed (CC0, CC BY, CC BY-SA, or Public Domain Mark) and was downloaded from the Internet Archive (archive.org).
No NC or ND licensed material is used.

Total duration of music/wav/01.wav..12.wav: **2612.815 s (43:32.815)**.

## Modifications (applies to the files in music/wav/)
All tracks were converted to 44.1 kHz / 16-bit / stereo PCM WAV with ffmpeg. Some tracks were **shortened with a fade-out**,
resampled, requantized to 16-bit with TPDF dither, and/or attenuated slightly to prevent clipping; each track's exact
processing is listed below and in tracklist.json. No loudness normalization or compression was applied. The original
downloads are kept unmodified in music/src/. For the CC BY / CC BY-SA tracks, these changes are indicated here as the license
requires. Adapted versions of the CC BY-SA track (01) remain under CC BY-SA 3.0.

## Tracks

### 01. Bastardo sigiloso
- Artist / performers: Chubby Mosh (David Fau & Javier Ribes)
- License: [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/)
- Source: <https://archive.org/details/chubbymoshelsegundo> (file: <https://archive.org/download/chubbymoshelsegundo/06Bastardosigiloso.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: bit-exact decode (no trim, no resample)
- Duration in test album: 223.829 s
- Notes: Album 'Chubby Mosh el segundo'. Stand-in for 'Death on Two Legs' (3:43): loud modern rock/metal master (~-6 LUFS), dense distorted guitars + drums; very wide stereo. Uploaded by the artist (David Fau).

### 02. L'art de toucher le clavecin - prelude (excerpt: 4th piece of the recording)
- Artist / performers: François Couperin (comp.); David Joseph Stith (harpsichord)
- License: [CC0 1.0 (public domain dedication)](https://creativecommons.org/publicdomain/zero/1.0/)
- Source: <https://archive.org/details/couperin_art_de_toucher> (file: <https://archive.org/download/couperin_art_de_toucher/couperin_art_de_toucher.flac>)
- Original format: FLAC 96 kHz/24-bit stereo
- Changes: trim 196.40-264.80 s, 10 ms fade-in, 0.3s fade-out, resample 96000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither
- Duration in test album: 68.400 s
- Notes: Stand-in for 'Lazing on a Sunday Afternoon' (1:08). Solo harpsichord (Flemish 3-choir harpsichord, Gerald Self 2020): bright plucked transients, rich HF. Excerpt 196.40-264.80 s of the 843 s recording, cut at natural silences between pieces. Uploaded by the performer.

### 03. We Wanna Rock
- Artist / performers: Чокнутый Пропеллер (Choknuty Propeller)
- License: [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)
- Source: <https://archive.org/details/SCL114> (file: <https://archive.org/download/SCL114/Choknuty_Propeller_-_We_Wanna_Rock.flac>)
- Original format: FLAC 44.1 kHz/24-bit stereo
- Changes: trim 0.00-190.0 s, 3s fade-out, 24-bit->16-bit TPDF dither; gain -0.1 dB
- Duration in test album: 190.000 s
- Notes: Single, Southern City's Lab netlabel [SCL114], 2013. Stand-in for 'I'm in Love with My Car' (3:05): NYHC-style hardcore, two distorted guitars, shouted vocals, very loud (-6.3 LUFS, LRA 2 LU). Trimmed from 207.7 s with fade.

### 04. Faceblind
- Artist / performers: Anne's Cordial (song by Eva Kendrick)
- License: [CC BY 3.0 US](https://creativecommons.org/licenses/by/3.0/us/)
- Source: <https://archive.org/details/AnnesCordialLiveAtTheNave> (file: <https://archive.org/download/AnnesCordialLiveAtTheNave/1FaceblindevaKendrick.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: trim 0.00-173.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- Duration in test album: 173.000 s
- Notes: Live at The Nave, 2008-07-27 (ARTSomerville, prod. Randy Winchester). Stand-in for 'You're My Best Friend' (2:50): female vocal trio with tight harmonies, piano/guitar/harp. Trimmed from 201.8 s with fade.

### 05. Leave Ruin
- Artist / performers: Strand of Oaks
- License: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
- Source: <https://archive.org/details/StrandOfOaksAtTheNave> (file: <https://archive.org/download/StrandOfOaksAtTheNave/1-LeaveRuin.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: trim 0.00-207.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- Duration in test album: 207.000 s
- Notes: Live at The Nave, 2008-08-08 (ARTSomerville). Stand-in for '39 (3:25): acoustic guitar + vocals, piano, harmony vocals, cymbal; quiet (-23.7 LUFS, LRA 16 LU) so stresses low-level coding. Trimmed from 235.6 s with fade.

### 06. Messiah, HWV 56: "Hallelujah"
- Artist / performers: G. F. Handel (comp.); Ottery St Mary Choral Society, cond. Malcolm Matthews (recorded by Gary Sherman)
- License: [CC0 1.0 (public domain dedication)](https://creativecommons.org/publicdomain/zero/1.0/)
- Source: <https://archive.org/details/HandelsMessiah> (file: <https://archive.org/download/HandelsMessiah/Zm.%20Hallelujah.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: bit-exact decode (no trim, no resample)
- Duration in test album: 244.000 s
- Notes: Live, March 2018. Stand-in for 'Sweet Lady' slot (4:01) / the album's choral layering: large SATB choir + orchestra + trumpets & timpani, wide stereo. Bit-exact.

### 07. Peer Gynt Suite No. 1, Op. 46: IV. In the Hall of the Mountain King
- Artist / performers: Edvard Grieg (comp.); Czech National Symphony Orchestra (Musopen)
- License: [Public Domain Mark 1.0 (Musopen Kickstarter release)](https://creativecommons.org/publicdomain/mark/1.0/)
- Source: <https://archive.org/details/MusopenCollectionAsFlac> (file: <https://archive.org/download/MusopenCollectionAsFlac/Greig_PeerGynt/EdvardGrieg-PeerGyntSuiteNo.1Op.46-04-InTheHallOfTheMountainKing.flac>)
- Original format: FLAC 48 kHz/24-bit stereo
- Changes: resample 48000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither; gain -0.63 dB
- Duration in test album: 154.091 s
- Notes: Musopen Kickstarter recording (2012), released to the public domain. Stand-in for 'Seaside Rendezvous' (2:13): pizzicato/staccato, huge crescendo from ppp to tutti with brass, cymbals and bass drum; LRA 24.8 LU (largest dynamic range in the set). Resampled; -0.63 dB gain to avoid inter-sample clipping after resampling.

### 08. Dracula pt. 1
- Artist / performers: Balthasar
- License: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
- Source: <https://archive.org/details/OMR002> (file: <https://archive.org/download/OMR002/02_-_Dracula_pt_1.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: bit-exact decode (no trim, no resample)
- Duration in test album: 490.937 s
- Notes: EP 'Men And Monsters' (Open Metal Records OMR002, 2012). Stand-in for 'The Prophet's Song' (8:17): 8:11 multi-section progressive metal epic (guitars, keyboards, vocals, programmed drums). Bit-exact.

### 09. Since You've Gone
- Artist / performers: Sandi Hammond
- License: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
- Source: <https://archive.org/details/SandiHammondAtTheNave> (file: <https://archive.org/download/SandiHammondAtTheNave/04-SinceYouveGone.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: trim 0.00-220.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- Duration in test album: 220.000 s
- Notes: Live at The Nave, 2008-05-02 (ARTSomerville). Stand-in for 'Love of My Life' (3:38): keyboard/piano-based singer-songwriter, classically trained solo soprano (item tagged vocal/jazz/piano; exact per-track instrumentation not verified by listening). Trimmed from 239.4 s with fade.

### 10. Cantabile in D major, Op. 17 (MS 109)
- Artist / performers: Niccolò Paganini (comp.); Altschuler-Caplan Duo (violin & classical guitar, Aaron Larget-Caplan, guitar)
- License: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
- Source: <https://archive.org/details/Altschuler-caplanDuo542008> (file: <https://archive.org/download/Altschuler-caplanDuo542008/1-NiccolPaganini-CantabileInDMajorOp.17.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: trim 0.00-208.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- Duration in test album: 208.000 s
- Notes: Live 2008-05-04 (ARTSomerville). Stand-in for 'Good Company' (3:26): acoustic plucked nylon-string guitar + solo violin, quiet (-20.7 LUFS). Trimmed from 228.4 s with fade.

### 11. The Magic Flute (Die Zauberflöte), K. 620: Overture
- Artist / performers: W. A. Mozart (comp.); Czech National Symphony Orchestra (Musopen)
- License: [Public Domain Mark 1.0 (Musopen Kickstarter release)](https://creativecommons.org/publicdomain/mark/1.0/)
- Source: <https://archive.org/details/MusopenCollectionAsFlac> (file: <https://archive.org/download/MusopenCollectionAsFlac/Mozart_MagicFluteOverture/WolfgangAmadeusMozart-MagicFluteOverture.flac>)
- Original format: FLAC 48 kHz/24-bit stereo
- Changes: trim 0.00-357.0 s, 4s fade-out, resample 48000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither
- Duration in test album: 357.000 s
- Notes: Musopen Kickstarter recording (2012). Stand-in for 'Bohemian Rhapsody' (5:55): solemn brass chords (trombones), soft adagio, then fast fugal allegro with tutti; strong transients and wide dynamics. Trimmed from 400.8 s with 4 s fade; resampled.

### 12. Messiah, HWV 56: "The Lord gave the word"
- Artist / performers: G. F. Handel (comp.); Ottery St Mary Choral Society, cond. Malcolm Matthews (recorded by Gary Sherman)
- License: [CC0 1.0 (public domain dedication)](https://creativecommons.org/publicdomain/zero/1.0/)
- Source: <https://archive.org/details/HandelsMessiah> (file: <https://archive.org/download/HandelsMessiah/Zf.%20The%20Lord%20gave%20the%20word.flac>)
- Original format: FLAC 44.1 kHz/16-bit stereo
- Changes: bit-exact decode (no trim, no resample)
- Duration in test album: 76.558 s
- Notes: Live, March 2018. Stand-in for 'God Save the Queen' (1:11): short full chorus + orchestra. Bit-exact.

## Attribution lines (short form)

- "Bastardo sigiloso" by Chubby Mosh (David Fau & Javier Ribes), CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/), via https://archive.org/details/chubbymoshelsegundo. Unmodified audio (decoded to WAV).
- "L'art de toucher le clavecin - prelude (excerpt: 4th piece of the recording)" by François Couperin (comp.); David Joseph Stith (harpsichord), CC0 1.0 (public domain dedication) (https://creativecommons.org/publicdomain/zero/1.0/), via https://archive.org/details/couperin_art_de_toucher. Modified: trim 196.40-264.80 s, 10 ms fade-in, 0.3s fade-out, resample 96000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither
- "We Wanna Rock" by Чокнутый Пропеллер (Choknuty Propeller), CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/), via https://archive.org/details/SCL114. Modified: trim 0.00-190.0 s, 3s fade-out, 24-bit->16-bit TPDF dither
- "Faceblind" by Anne's Cordial (song by Eva Kendrick), CC BY 3.0 US (https://creativecommons.org/licenses/by/3.0/us/), via https://archive.org/details/AnnesCordialLiveAtTheNave. Modified: trim 0.00-173.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- "Leave Ruin" by Strand of Oaks, CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/), via https://archive.org/details/StrandOfOaksAtTheNave. Modified: trim 0.00-207.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- "Messiah, HWV 56: "Hallelujah"" by G. F. Handel (comp.); Ottery St Mary Choral Society, cond. Malcolm Matthews (recorded by Gary Sherman), CC0 1.0 (public domain dedication) (https://creativecommons.org/publicdomain/zero/1.0/), via https://archive.org/details/HandelsMessiah. Unmodified audio (decoded to WAV).
- "Peer Gynt Suite No. 1, Op. 46: IV. In the Hall of the Mountain King" by Edvard Grieg (comp.); Czech National Symphony Orchestra (Musopen), Public Domain Mark 1.0 (Musopen Kickstarter release) (https://creativecommons.org/publicdomain/mark/1.0/), via https://archive.org/details/MusopenCollectionAsFlac. Modified: resample 48000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither
- "Dracula pt. 1" by Balthasar, CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/), via https://archive.org/details/OMR002. Unmodified audio (decoded to WAV).
- "Since You've Gone" by Sandi Hammond, CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/), via https://archive.org/details/SandiHammondAtTheNave. Modified: trim 0.00-220.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- "Cantabile in D major, Op. 17 (MS 109)" by Niccolò Paganini (comp.); Altschuler-Caplan Duo (violin & classical guitar, Aaron Larget-Caplan, guitar), CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/), via https://archive.org/details/Altschuler-caplanDuo542008. Modified: trim 0.00-208.0 s, 3s fade-out, requantized to 16-bit with TPDF dither (fade)
- "The Magic Flute (Die Zauberflöte), K. 620: Overture" by W. A. Mozart (comp.); Czech National Symphony Orchestra (Musopen), Public Domain Mark 1.0 (Musopen Kickstarter release) (https://creativecommons.org/publicdomain/mark/1.0/), via https://archive.org/details/MusopenCollectionAsFlac. Modified: trim 0.00-357.0 s, 4s fade-out, resample 48000->44100 (swr, filter_size=128, phase_shift=10, cutoff=0.97), 24-bit->16-bit TPDF dither
- "Messiah, HWV 56: "The Lord gave the word"" by G. F. Handel (comp.); Ottery St Mary Choral Society, cond. Malcolm Matthews (recorded by Gary Sherman), CC0 1.0 (public domain dedication) (https://creativecommons.org/publicdomain/zero/1.0/), via https://archive.org/details/HandelsMessiah. Unmodified audio (decoded to WAV).

## Caveats
- License information is as declared on each archive.org item page (licenseurl field) by the uploader. Uploaders are the
  artist/label/recordist: Chubby Mosh (David Fau), Open Metal Records (Balthasar), Southern City's Lab netlabel (SCL114),
  ARTSomerville producer Randy Winchester (The Nave live sessions: tracks 4, 5, 9, 10), the performer David Joseph Stith
  (Couperin), the recordist Gary Sherman (Messiah), and the Musopen Kickstarter collection (orchestral tracks 7, 11).
- The Musopen recordings are Musopen's 2012 Kickstarter recordings, which Musopen released to the public domain.
- Musical works by Couperin, Handel, Grieg, Paganini and Mozart are in the public domain; the recordings' licenses are as above.
