## Omsi-Hub (NexiconDK)

Vores egen udgave af Omsi-Hub med disse ændringer.

### Multiplayer og dedicated server
- Værtens AI-busser viser nu både linje **og destination** hos spillerne. Før stod kun linjen på skiltet, fordi spillerens kopi af bussen ikke fik hof-filen med, som destinationsteksten hentes fra. Det er kun spillerne, der skal have den nye version; serveren skal ikke ændres.

- **Vælg vagt inde i spillet på serveren**: En spiller, der er tilsluttet en server uden at have valgt tur i launcheren, fik "No timetable on this map". Nu kan man vælge og skifte tur inde i spillet (Timetable i menuen), ud fra serverens ur. Lægger serveren en kopi af kortets `TTData` i sin egen indholdsmappe (fx `openOMSI-HUB\maps\Ahlheim 5\TTData`), får alle, der tilslutter sig, den køreplan med, og så kører alle efter serverens køreplan.

- **Alle kører efter serverens køreplan**: Serveren pakker den køreplan, den selv kører efter (kortets `TTData`, eller kopien i serverens indholdsmappe hvis der er en), og deler den på web-porten (`/ttdata.zip`, slås fra med `share_timetable = 0` i `server.cfg`). Når man vælger serveren under Multiplayer, henter launcheren den (gemmes til næste gang i `~/.openomsi/server-timetables`) og viser serverens linjer og ture under "Tour" og "Work shift". Spillet får samme køreplan med, når man starter. Ingen behøver at kopiere `TTData`. Vælger man "Host" eller en vens kode, bruges ens egen køreplan igen.

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
- **Ekstra tekst på skiltet**: Destinationsvinduet (fx "Destination" i Alt-menuen) har nu to faner, "Destination" og "Extra text", så man kan skifte mellem dem i samme vindue. Den findes også under køretøjet i spillets menu. Skriv en tekst på to linjer, fx "EKSTRABUS" eller "Via Hovedbanen", og tryk "Turn on". Så skifter den med destinationen på bussens skilte, som standard hvert 4. sekund (antal sekunder kan vælges). Den bliver ved, også når du skifter destination eller tur, indtil du trykker "Turn off". IBIS'en og passagererne mærker intet til det: destinationen, dens kode og navn er de samme hele tiden. Et rullebånd kan ikke vise ekstra tekst.

- **Egen tekst og ekstra tekst virker nu på Krüger-matricerne**: En tekst skrevet i destinationsvinduet blev også lagt på den plads i hof-filen, hvor Krüger-skiltene har et billede af destinationen. Så ledte skiltet efter en billedfil med tekstens navn (fx `Anzeigen\Krueger\Aarhus Rtb`), fandt ingen og viste ingenting. Nu får billedpladsen ingen tekst, og det samme gælder pladser, som hof-filens destinationer næsten altid lader stå tomme, fx indstikskiltet. Teksten står igen på skiltet, og det gælder også "Extra text".

### Line and tour (i spillets menu)
- **Turens side viser nu hele turen** (som rutebogen fra openOMSI PR #2936): Vælger man en tur, vises turens ture under turlisten, hver med start (åben ring), slut (fyldt prik), tiderne, linjen og pausen til næste tur. Klik på en tur for at starte derfra. Øverst til højre står, hvor den valgte tur starter og slutter.
- **IBIS-boksen**: Over "Start trip" står den linje- og rutekode, man skal taste i IBIS'en til netop den tur, fremhævet ud fra bussens hof-fil, og linjens andre ruter står under. Har hof-filen ingen rute til turen, står det, og så vises linjen og destinationskoden i stedet.
- **Kort over ruten**: Midt på siden vises turens rute på et kort med vejene omkring og stoppestederne med tider. Første stop er en ring, sidste en fyldt prik, og det stop, man starter fra, er markeret. Kortet vises, når navigatoren har læst kortet, og kun hvis vinduet er bredt nok.

### Køreplanssedlen i førerpladsen
- **Køreplanssedlen virker igen** (`file_schedule`, papiret med turens stop og tider): HUB tegnede den i `~\.openomsi\cache\schedules`, men måtte ikke vise den, fordi et billede med fuld sti kun læses fra indholdsmapperne (OMSI og mods). Nu er netop den mappe tilladt, så sedlen vises som i main. Alt andet er stadig begrænset til indholdsmapperne.

### Til fods
- **Ctrl+Shift+G virker igen**: Når "Ability to get up" er slået til, rejser chaufføren sig nu altid fra sædet med Ctrl+Shift+G, som i vores main. Før blev tasten ignoreret, hvis Ctrl+Shift+G også stod i en `keyboard.cfg`, og så skete der ingenting. Er indstillingen slået fra, bruger tasten det, den er sat til i `keyboard.cfg`.

### Plugins
- **Plugins kan læse spillets data og hele ture** (openOMSI PR #3102 af Burst99325): `omsi.game.query` læser det samme som bussens scripts (skilte, køreplan, billetter osv.), også fra fx et afgangsskilt ved et stoppested. `omsi.game.queries()` viser, hvad der kan læses. `omsi.timetable.tour(linje, tur)` giver hele turen med alle stop og tider uden at tage den, og `omsi.duty.tour()` giver hele den tur, man kører. Plugins kan kun læse med dem, ikke ændre noget.
- **IBIS'en kommer tilbage, når et plugin stopper** (openOMSI PR #2877 af Luc Ruigrok): Et plugin, der overlader IBIS'en til chaufføren (`omsi.duty.set_auto_ibis(false)`), giver den nu tilbage til spillet, når pluginet stopper eller slås fra. Før blev IBIS'en ved med ikke at blive udfyldt. Har to plugins hver taget den, giver hvert kun sin egen tilbage. Shift+U siger nu "A plugin leaves the IBIS for you to type", så en tom IBIS ikke ligner en fejl.

### Bykortet (Shift+M)
- **Kortets indgangspunkter** (fra openOMSI PR #3074 af shamough1792): Indgangspunkterne, hvor bussen kan sættes ind på kortet, vises nu som orange prikker med deres navn. Klik på en prik eller et navn, så sættes et punkt dér med indgangspunktets navn, og ruten går derhen. Pin-værktøjet behøver ikke være slået til.
- **Slet et punkt direkte på kortet**: Hold musen over et af dine punkter, så kommer der et lille kryds over det. Klik på krydset for at fjerne netop det punkt. Resten bliver, og ruten lægges om. Højreklik og kortet med punkterne virker som før.

### Bus Company
- **Gennemgangen af bussen ("Walk-around check") og noten til næste chauffør kan klikkes med det samme**: De tager selv musen, når de åbner, og giver den tilbage, når de er sendt. Før skulle man først trykke F10, og det stod ingen steder. Vagtstrimlen viser nu også "F10: use the panel with the mouse", når panelerne ikke har musen.
