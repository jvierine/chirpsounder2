# Marieluise CAD files

Mechanical CAD files for the chirpsounder2 receiver hardware, provided by
Marieluise Schmitt Gran in email on 2026-05-21 and 2026-05-22.

The parts relate to the receiver hardware described in Marieluise's thesis:
an MLA-30+ active-loop receive antenna, fiberglass mast/support hardware, and
the USRP N200 receiver with a Leo Bodnar GPSDO timing source.

## Active-loop antenna assembly

- `antenna_assembly.SLDASM`: SolidWorks assembly for the antenna hardware.
- `alu_loop.SLDPRT`: aluminum loop antenna element.
- `fiberglass_rod.SLDPRT`: fiberglass support rod/mast component.
- `holding_bracket.SLDPRT`: bracket for holding the loop/support structure.
- `mounting_bracket.SLDPRT`: mounting bracket, likely for attaching the
  antenna/preamp hardware to the mast or support structure.
- `preamp.SLDPRT`: preamplifier/MLA-30+ representation or mounting model.
- `standoff_clamp.SLDPRT`: lower/primary standoff clamp component.
- `standoff_clamp_top.SLDPRT`: top half of the standoff clamp.
- `u_bolt.SLDPRT`: U-bolt used with the mounting hardware.
- `lid.SLDPRT`: lid/cover part for the antenna or preamp mechanical assembly.

## GPSDO/USRP mount

- `GPSDOUSRPmount.step`: STEP model for mounting the GPSDO with the USRP.
- `GPSDOUSRPmountstl.stl`: STL export of the GPSDO/USRP mount, suitable for
  viewing or 3D printing.

## Notes

These files are the original attachments from the emails. The SolidWorks files
are native CAD sources; the STEP/STL files are interchange/printable exports for
the GPSDO/USRP mount.
