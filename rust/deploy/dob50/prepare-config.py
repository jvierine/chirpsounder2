"""DOB-only configuration overlay; original profiles are preserved."""
from pathlib import Path
import json
home=Path.home()
assert home.name=='tgo'
root=home/'.config/chirpsounder2'
c=json.loads((home/'.config/iono-prn/Montelibretti.json').read_text())
c.update(data_dir='/dev/shm/hf50',sample_rate=50_000_000,center_freq=25_000_000.)
c['frequencies_hz']=[float(i*100_000) for i in range(10,301)]
c['code_indices']=[c['code_indices'][0]]*len(c['frequencies_hz'])
assert len(set(c['code_indices']))==1 and c['period_s']>=len(c['frequencies_hz'])*c['dwell_s']
c['configuration_reference']+='; DOB-only experimental native 50 MHz sc8 capture; extend existing 100 kHz frequency grid through 30 MHz; upper program requires on-air check'
(root/'Montelibretti50.json').write_text(json.dumps(c,indent=2)+'\n')
c=json.loads((root/'receiver.json').read_text())
c.update(data_dir='/dev/shm/hf50',sample_rate=50_000_000.,center_freq=25_000_000.,
         detector_samples_per_block=8_000_000,detector_worker_count=2)
(root/'receiver50.json').write_text(json.dumps(c,indent=2)+'\n')
c=json.loads((root/'spectrum.json').read_text())
c.update(data_dir='/dev/shm/hf50',sample_rate=50_000_000.,center_freq=25_000_000.,
         output_dir='/data0/dob50-diagnostics')
(root/'spectrum50.json').write_text(json.dumps(c,indent=2)+'\n')
