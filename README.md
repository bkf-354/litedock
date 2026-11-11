# LiteDock

Dock ringan ala macOS untuk Windows 11. Ditulis dengan Rust native (tanpa Electron/WebView) dan digambar dengan renderer perangkat lunak, jadi tidak ada proses browser di latar belakang.

## Fitur

| Area | Isi |
|---|---|
| Dock | Ikon aplikasi berjalan + yang disematkan, tombol Start, titik indikator aplikasi aktif |
| Zoom | Ikon membesar saat kursor lewat (kurva lonceng), ikon yang zoom keluar dari latar dock |
| Animasi | Zoom dan geser halus, pantulan saat membuka aplikasi, animasi minimize/maximize ala genie |
| Jadwal salat | Dua modul terpisah di sebelah Start: info waktu (ikon berwarna per waktu salat, nama, jam) dan hitung mundur (cincin kemajuan + sisa waktu); klik untuk semua waktu. Dihitung offline (Kemenag RI, MWL, ISNA, Mesir, Umm al-Qura, Karachi). Lokasi: deteksi otomatis (Lokasi Windows, cadangan IP via HTTPS), 19 kota Indonesia, atau lintang/bujur manual |
| Notifikasi salat | Kartu ringkas berwarna sesuai waktu salat, keluar dari dock tepat saat masuk waktu (bunyi opsional); tidak muncul saat aplikasi layar penuh |
| Modul | Hari/tanggal bertumpuk (tanpa latar) + jam, Wi-Fi (kekuatan sinyal, Ethernet), baterai, tombol ikon tray. Semua ikon digambar vektor bergaya Fluent Windows 11 dan satu skala dengan teks |
| Gaya latar | Mac, Gelap, Surface (ikut tema Windows), Terang, Bening. Kepekatan, sudut, ukuran, zoom, jarak dari bawah, jarak label ke ikon bisa diatur. Jendela Pengaturan ikut bergaya sama dengan dock |
| Ikon | Aplikasi Store/MSIX (mis. WhatsApp) memakai ikon tile asli; bila ikon tidak ditemukan dibuat ubin berwarna dengan huruf awal aplikasi |
| Taskbar asli | Bisa disembunyikan; dock menyisakan ruang sehingga jendela maksimal tidak menutupinya |
| Panel detail | Klik jam/tanggal membuka kalender (ganti bulan dengan panah atau tombol kiri/kanan), klik kartu salat membuka jadwal lengkap hari itu, klik Wi-Fi membuka panel jaringan (Wi-Fi, Mode pesawat, Hotspot seluler, daftar jaringan tersedia, sambung/putuskan), klik baterai membuka panel daya (Penghemat baterai, Mode daya: hemat/seimbang/performa). Panel mengembang dari dock, ditutup dengan Esc atau klik di luar |
| Sembunyi otomatis | Dock meluncur turun/naik dengan easing (bukan menghilang kaku). Pengaturan > Sistem: Tidak pernah / Saat aplikasi layar penuh / Saat jendela maksimal, layar penuh, atau menutupi dock (bawaan) / Selalu (muncul saat kursor menyentuh tepi bawah layar) |
| Ukuran | Pengaturan > Tampilan: slider "Ukuran ikon Wi-Fi, baterai, tray, jam" dan "Ukuran teks dock" (60-160%). Info salat: info + hitung mundur, hanya info, atau hanya hitung mundur |
| Ikon tray | LiteDock menjadi host tray (seperti RetroBar): ikon aplikasi latar belakang tampil di panel tray LiteDock (klik kiri = buka, klik kanan = menu), bukan di taskbar Windows. Aktif bila taskbar asli disembunyikan; bisa dimatikan di Pengaturan > Elemen |
| Aman | Taskbar otomatis dipulihkan saat keluar, saat crash terdeteksi di start berikutnya, dan lewat pintasan "Pulihkan Taskbar Windows" |

## Memasang

Jalankan `LiteDock-Setup-1.9.0.exe` (tanpa hak admin, terpasang per pengguna). Windows SmartScreen bisa memberi peringatan karena berkas belum ditandatangani digital: pilih "More info" lalu "Run anyway".

## Memakai

- Klik kanan dock: pengaturan, intip taskbar, keluar. Klik kanan ikon: buka baru, sematkan/lepas, tutup jendela.
- Klik tengah ikon: buka jendela baru.
- Menjalankan LiteDock lagi saat sudah aktif akan membuka jendela pengaturan.
- Layar penuh tidak terdeteksi? Jalankan `LiteDock.exe --diag`, arahkan fokus ke aplikasi itu dalam 6 detik, lalu kirim `diag.txt` yang terbuka.
- Taskbar tersembunyi dan butuh dipulihkan manual? Jalankan `LiteDock.exe --restore` atau pintasan "Pulihkan Taskbar Windows" di Start Menu.

## Jika bermasalah

- LiteDock menutup sendiri? Buka `%APPDATA%\LiteDock\crash.txt` dan kirim isinya (berisi jenis galat dan langkah terakhir yang dikerjakan).
- Layar penuh tidak terdeteksi? `LiteDock.exe --diag`, arahkan fokus ke aplikasi itu dalam 6 detik.
- Taskbar tidak muncul? `LiteDock.exe --restore`.

## Build dari sumber

```
cargo build --release
```

Installer: salin `target\release\litedock.exe` ke `installer\LiteDock.exe`, lalu `makensis installer\litedock.nsi`. Workflow GitHub Actions di `.github/workflows/build.yml` melakukan keduanya otomatis di Windows.

## Baru di v1.9.0

- Tampilan ala macOS: font Inter (pengganti SF Pro yang boleh disertakan) menjadi default, palet panel grafit netral, panel dan banner dengan sudut membulat, kartu tanpa garis tepi, tombol kapsul halus, switch hijau dan slider tipis di Pengaturan.
- Pusat Kontrol dirombak: ubin Wi-Fi, Bluetooth, Pesawat, Hotspot berikon bulat (biru saat aktif) dengan status, dan slider suara pil tebal dengan ikon speaker di dalamnya (ketuk ikon = bisukan).
- Kalender: hari ini ditandai lingkaran merah, judul dan panah bulan ala macOS. Cuaca: suhu besar dan tipis. Musik: sampul membulat, judul/artis, kontrol polos. Baterai: cincin tipis (hijau saat mengisi, merah saat hampir habis) dan baris Penghemat baterai dengan switch.
- Banner notifikasi: latar netral, ikon squircle berwarna dengan glif putih, keterangan waktu redup di kanan. Label nama aplikasi di atas dock berupa kapsul gelap tembus.
- Ikon Windows (Start): warna ubin bisa dipilih Terang atau Gelap di Pengaturan > Elemen.
- Ikon sistem (Wi-Fi, baterai, dll.) kini vektor bergaya SF Symbols secara default; glif Segoe Fluent Icons tetap bisa dipakai lewat Pengaturan > Elemen > "Ikon sistem gaya Windows".
- Ikon aplikasi lebih tajam: pengubah ukuran memakai filter Lanczos-3, ikon sumber kecil (32-64 px) yang diperbesar dipertajam tepinya, dan ikon kecil yang diletakkan shell di tengah kanvas 256 px dipotong lalu diperbesar dengan benar.

Font Inter (SIL Open Font License 1.1) disertakan sebagian (aksara Latin, bobot Light/Regular/Medium/SemiBold/Bold); lisensi ada di `assets/fonts/Inter-LICENSE.txt`.

## Baru di v1.8.0

- Dock simetris saat diam: ikon Start kini ubin ukuran penuh yang sama dengan ikon aplikasi, jarak dari tepi kiri dan kanan dihitung sampai isi yang terlihat (bukan tepi slot), ikon aplikasi selalu di tengah vertikal, dan posisi dibulatkan ke piksel utuh.
- Ubin persegi membulat pada ikon aplikasi digambar vektor pada 256 px (bukan pada ukuran ikon sumber yang kecil), jadi tepi dan gradien tidak lagi blur/pecah.
- Modul salat digabung menjadi satu: ikon dua warna, nama singkat, jam, titik tengah, hitung mundur (`ASH 14:54 · 02:14:33`). Ikon salat tetap berwarna dua warna di dock, panel jadwal, dan banner. Isi modul bisa dipilih di Pengaturan > Elemen.
- Ikon baterai dan persentasenya terpisah (Pengaturan > Ukuran): bisa menampilkan ikon saja, persen saja, atau keduanya.
- Ukuran ikon aplikasi dan ukuran ikon tray/modul sistem kini dua pengaturan terpisah (tab Ukuran baru). Pengaturan lama dimigrasikan otomatis.
- Ikon baterai, Wi-Fi, Ethernet, tray, Recycle Bin, daya, pusat kontrol, dan musik memakai glif Segoe Fluent Icons bawaan Windows 11 (cadangan Segoe MDL2 Assets, lalu ikon vektor bila keduanya tidak ada). Baterai berwarna hijau saat mengisi, oranye di bawah 30%, merah di bawah 15%.
- Ikon cuaca lebih berwarna: matahari emas-oranye, awan biru-putih, hujan biru, petir kuning.
- Teks panel dan Pengaturan lebih ringan: semibold hanya untuk judul, teks kecil regular, angka besar semilight.
- Banner Caps Lock, clipboard, baterai, dan jaringan dibuat lebih kecil dan proporsional; banner salat dan aplikasi sedikit lebih besar.
- Banner notifikasi aplikasi (WhatsApp dll.): pemantauan jendela toast diganti polling 4x per detik dengan pembacaan UI Automation berulang, sehingga tidak bergantung pada satu peristiwa jendela. Log diagnosis (tanpa isi pesan) ada di `%APPDATA%\LiteDock\toast.log`.

## Baru di v1.7.0

- Dock di atas aplikasi layar penuh: sentuh tepi bawah untuk memunculkan; lama tampil 5-15 detik (Pengaturan > Sistem).
- Jarak antar modul (Wi-Fi, baterai, tanggal, jam, dll.) seragam dan bisa diatur; padding kiri/kanan dock bisa diatur; warna teks dock bisa dipilih (Pengaturan > Teks).
- Tab Modul: tampilkan/sembunyikan dan atur urutan semua modul dengan tombol naik/turun.
- Modul baru: Recycle Bin (klik buka, klik kanan kosongkan), File Explorer (klik kanan folder cepat), Daya (kunci, tidur, mulai ulang, matikan, keluar; konfirmasi untuk tindakan merusak), Pusat Kontrol (volume, Wi-Fi, Bluetooth, mode pesawat, hotspot), Pemutar Musik (kontrol media Windows), Cuaca (Open-Meteo).
- Ikon salat berwarna tanpa kotak latar; kapsul latar info/hitung mundur salat opsional; hitung mundur punya bilah kemajuan tipis.
- Ikon baterai lebih jelas: garis dan isi hitam (isi hijau saat mengisi, oranye/merah saat rendah).
- Ikon aplikasi berupa glif transparan otomatis dibungkus ubin persegi membulat ala iOS/macOS dengan warna serasi; mip ikon lebih rapat dan dasar 256 px agar tajam pada skala berapa pun.
- Banner pop-up selain salat: Caps/Num Lock, clipboard, notifikasi aplikasi (WhatsApp dll., dibaca dari banner Windows lewat UI Automation), baterai, jaringan (Pengaturan > Banner).
- Panel lebih sederhana (monokrom + satu warna aksen); panel tray tetap terbuka saat klik kanan ikon sampai menu aplikasi selesai.
- Menu Start (tombol Windows) tidak lagi memunculkan taskbar asli: LiteDock menyembunyikannya kembali selama menu terbuka.

## Baru di v1.6.0

- Komposisi dock dirapikan: Wi-Fi, baterai, jam, dan teks memakai satu satuan ukuran dan jarak yang sama. Jam satu baris berdampingan, contoh `Min 08 Nov 15:28` (tanpa tumpukan).
- Info salat singkat: `SUB / DHU / ASH / MGH / ISY` + jam; hitung mundur format `jj:mm:dd` yang berjalan tiap detik.
- Pilihan font di Pengaturan > Teks: Segoe UI (bawaan Windows), Roboto, atau Inter (disertakan, tidak perlu diinstal), ketebalan Reguler/Tebal untuk teks dock termasuk info salat, dan ukuran teks dock.
- Menambah/menyalakan elemen dock jauh lebih ringan: repaint jendela Pengaturan turun dari sekitar 28 ms menjadi 1-2 ms, bayangan dock tidak digambar ulang saat dock melebar/menyempit, status jaringan dihitung di thread latar, cache teks diperbesar, dan frame pertama setelah diam tidak lagi melompat ke akhir animasi.
- Sembunyi otomatis naik/turun dengan easing yang konsisten (perbaikan lonjakan waktu frame pertama).
- Panel dikecilkan sekitar 14%.
- Panel tray ringkas: ikon kecil dan tajam (ukuran asli ikon, tanpa pembesaran/blur).

Font Roboto (Apache License 2.0) disertakan sebagian (aksara Latin); lisensi ada di `assets/fonts/Roboto-LICENSE.txt`.

## Baru di v1.5.0

- Ikon Wi-Fi, baterai, jam, teks: satu sistem ukuran, ikon vektor ala Windows 11, slider ukuran teks dock.
- Hari/tanggal bertumpuk tanpa latar; salat dipisah jadi modul info dan modul hitung mundur dengan ikon berwarna; notifikasi salat lebih ringkas.
- Sembunyi otomatis meluncur naik/turun.
- Panel dirombak: kartu jaringan terhubung, daftar jaringan langsung tampil dari cache lalu diperbarui pemindaian, status Wi-Fi/baterai di dock diperbarui tiap 5 detik. Wi-Fi memakai profil koneksi Windows (tidak lagi tertipu adapter virtual).
- Panel tray sendiri (host tray).
- Menambah/menyalakan modul di Pengaturan lebih mulus: tanpa registrasi ulang appbar dan tulis disk tiap geseran, animasi muncul dengan easing, isi modul dipotong dan dipudarkan saat muncul.

## Batasan v1.9.0

- Dock hanya di monitor utama; taskbar monitor kedua ikut disembunyikan.
- Latar belum memakai blur acrylic (kaca ditiru dengan gradasi tembus pandang).
- Deteksi lokasi via IP hanya akurat setingkat kota/ISP; Lokasi Windows lebih akurat (aktifkan Layanan Lokasi dan izin untuk aplikasi desktop). Zona waktu mengikuti pengaturan Windows.
- Banner notifikasi aplikasi membaca teks banner Windows lewat UI Automation; bergantung pada versi Windows dan bahasa tampilan, dan banner bawaan tetap muncul kecuali opsi sembunyikan (percobaan) diaktifkan. Cuaca membutuhkan internet dan lokasi di Pengaturan > Salat.
- Host tray: aplikasi yang sudah berjalan memasang ulang ikonnya lewat siaran `TaskbarCreated`; aplikasi yang tidak menanganinya perlu dijalankan ulang. Balon notifikasi dari ikon tray belum ditampilkan. Ikon sistem Windows (jaringan, volume, daya) tetap ditangani Explorer. Bila host tray dimatikan, tombol tray kembali mengintip taskbar asli.
- Animasi minimize/maximize berlaku untuk klik lewat dock (bukan tombol minimize di jendela itu sendiri).
- Posisi dock hanya di bawah.
