## Omsi-Hub (NexiconDK)

Vores egen udgave af Omsi-Hub med disse ændringer.

### Multiplayer og dedicated server
- Værtens AI-busser viser nu både linje **og destination** hos spillerne. Før stod kun linjen på skiltet, fordi spillerens kopi af bussen ikke fik hof-filen med, som destinationsteksten hentes fra. Det er kun spillerne, der skal have den nye version; serveren skal ikke ændres.

### Livekort på den dedikerede server
- Serveren viser nu selv et livekort i browseren på `http://<serverens ip>:<web_port>/kort` (fx `http://192.168.1.169:27026/kort`). Det kræver `share_positions = 1` i `server.cfg`. Forsiden af web-porten har et link til kortet.
- Kortet tegnes ud fra kortets egne filer: veje, fortove, skinner, stoppesteder og hele køreplanen. Serveren lægger det sammen, når den starter (det tager et øjeblik), og det virker på alle kort.
- **Følg en bus** (klik på den på kortet eller i listen): turens rute tegnes, den kørte del gråt og resten i bussens farve, med stoppestederne og deres afgangstider. En boks øverst viser **næste stop** med afgangstid, hvor længe der er til, og hvor langt der er.
- **Linjer**: slå linjer til og fra i listen (eller "Vis alle"), så deres ruter og stoppesteder tegnes i hver deres farve.
- **AI-busserne** kører på kortet i deres linjes farve med linjenummer og antal passagerer. Klik på en for at se dens tur, forsinkelse, næste stop og vognnummer. De kan slås fra under "Vis på kortet".
- **Passagerer**: spillernes og AI-bussernes antal passagerer vises (`passengers` i `GET /players`).
- Til andre værktøjer: `GET /kort/kort.json` (kortet og køreplanen), `GET /kort/rute/<n>` (en turs rute) og `GET /kort/busser` (AI-busserne lige nu).

### Skilte
- **Egen tekst på skiltet viser ikke længere "Einsatzwagen"**: En egen tekst i destinationsvinduet fik en kode over 1000 (i Ahlheim 5: 1015). Krüger-matricerne (fx Urbino) tager sådan en kode for et indstikskilt nr. 15, som i Ahlheim er "Einsatzwagen". Nu får teksten en ledig kode under 1000 (i Ahlheim: 917) og står på skiltet, som den er skrevet.
- **Ekstra tekst på skiltet**: Ny menu "Extra text" under køretøjet i spillets menu, lige under "Destination display". Skriv en tekst på to linjer, fx "EKSTRABUS" eller "Via Hovedbanen", og tryk "Turn on". Så skifter den med destinationen på bussens skilte, som standard hvert 4. sekund (antal sekunder kan vælges). Den bliver ved, også når du skifter destination eller tur, indtil du trykker "Turn off". IBIS'en og passagererne mærker intet til det: destinationen, dens kode og navn er de samme hele tiden. Et rullebånd kan ikke vise ekstra tekst.

### Til fods
- **Ctrl+Shift+G virker igen**: Når "Ability to get up" er slået til, rejser chaufføren sig nu altid fra sædet med Ctrl+Shift+G, som i vores main. Før blev tasten ignoreret, hvis Ctrl+Shift+G også stod i en `keyboard.cfg`, og så skete der ingenting. Er indstillingen slået fra, bruger tasten det, den er sat til i `keyboard.cfg`.
