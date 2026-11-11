# KHO receiver cable handover

The new MIC-780W receiver is prepared to use the existing N200, antenna,
10 MHz reference and PPS. Move only the USRP Ethernet cable from the old
`obliono` PC to any of the new PC's three unused Ethernet sockets. Leave the
new PC's existing management-network cable connected.

The spare ports form the `usrp0` active-backup bond at `192.168.10.3/24`.
The radio remains `192.168.10.2`, serial `E9R17S3UP`. The bond has no default
route and does not replace the management interface. One radio cable is used.

Both PCs run `kho-link-handover.service`. After a stable link change, the old
PC stops its scientific workers, uploads and acquisition. The new PC starts
`iono-kho.target`: acquisition, the 90-second RAM-ring cleaner, Digisonde,
SGO LFM, chirp detection, spectrum, Kiruna/Uppsala/Montelibretti PRN, Finnish
VERSO, browser arrays and priority publication. Moving the cable back reverses
the handover. Neither PC needs an interactive login to run its services.

The new recorder waits for synchronized system time, the radio's response and
the expected serial number. Processing and publication wait for fresh voltage.
The existing external-reference/PPS convention is retained; moving the PC does
not calibrate the receiver or establish reference lock. Check the recording
log and advancing sample timestamps after the physical move.

Science products stay in `/data0` on the new SSD. Raw voltage is only retained
in the 90-second RAM ring. Disk health is included in browser publications;
scientific HDF5 files are not silently deleted to make space. Priority bundles
use capped HTTP uploads, and full browser history retains its five-minute
publication cadence. The dedicated reverse-tunnel private key stays on the receiver.

## Install or reproduce

Use the reviewed Git revision and the station's existing VERSO HDF5 program.
The JSON configurations in `config/` preserve the active KHO sounder programs,
range windows, nominal timing correction and recorder-write grace periods.
The program checksum is checked by `install-user.sh`.

1. Run `install-dependencies.sh` as root on the new PC. Install Rust with
   rustup. Place Digital RF tag `2.6.14` in `~/src/digital_rf` and this checkout
   in `~/src/chirpsounder2`.
2. Run `build.sh` as `administrator`. It builds the Digital RF C library,
   recorder, UHD status reader and native Rust workers, then runs DSP tests.
3. Run `configure-network.sh` as root. It preserves the management interface
   and backs up the previous network configuration.
4. Run `install-user.sh` as `administrator`, then `install-guard.sh new` as
   root. Install the same reviewed guard on the old PC with
   `install-guard.sh old`. The old guard takes over boot control without
   restarting an already running receiver.

## Check the handover

On either PC, inspect `systemctl status kho-link-handover` and its journal.
On the new PC:

```sh
cat /sys/class/net/usrp0/carrier
systemctl --user status iono-kho.target iono-kho-acquisition
journalctl --user -u iono-kho-acquisition -n 40
systemctl --user --failed
```

Confirm fresh HDF5 products and advancing KHO observation timestamps on the
observatory. Before the cable move, a running handover guard with an inactive
scientific target is the expected state. Acquisition can be exercised without
the radio: it must wait, and stopping the target must cancel every queued worker.

Validation before the move: 89 native tests passed (three manual benchmarks
ignored); the cable state-machine tests passed; an existing KHO Uppsala HDF5
ionogram produced a priority bundle and 16 browser products with uploads
disabled. Live RF throughput and reference/PPS lock remain checks for the
physical handover. Access credentials are kept only in Juha's private notes.

## Remote access

Juha explicitly authorized a reverse SSH tunnel for the new KHO PC on
11 November 2026. The enabled user service `kho-reverse-tunnel.service`
starts at boot through user lingering and reconnects automatically. It is
independent of the USRP cable and the acquisition target.

Connect from Juha's Mac with `ssh kho-new`, or explicitly:

```sh
ssh -J juha-no -p 5559 administrator@127.0.0.1
```

The old PC remains on port 5558. The new listener is bound to juha.no's
loopback address only. Its dedicated `kho-new-tunnel` account allows remote
forwarding only to port 5559, with no shell/session access. Its dedicated
Ed25519 private key is generated and retained on the receiver. The juha.no
host key is pinned using the existing authenticated management connection.

Install the unit in `~/.config/systemd/user/` after provisioning the key and
server restrictions, then run `systemctl --user enable --now kho-reverse-tunnel`.
Inspect `journalctl --user -u kho-reverse-tunnel` to diagnose connectivity.
