"""Copy bounded cached IQ windows for cross-receiver comparison; no RF DSP."""
import sys,h5py,numpy as np
source,destination,station=sys.argv[1:]
rows=[6,8,10]
samples=[];carriers=[];values=[]
for epoch in [1794065939,1794065965]:
 with h5py.File(f'{source}/digisonde_code_matches-{epoch}.h5') as h:
  samples.extend(h['iq/sample_index'][rows]);carriers.extend(h['iq/carrier_hz'][rows]);values.extend(h['iq/baseband_complex32'][rows])
with h5py.File(destination,'w') as h:
 h.attrs['station']=station;h.attrs['iq_sample_rate_hz']=100000.;h.attrs['source_rows']=rows
 h.attrs['subset_note']='Exact windows copied from receiver-local cached IQ; no interpolation or receiver timing change'
 h.create_group('iq');h['iq/sample_index']=np.asarray(samples,dtype=np.uint64);h['iq/carrier_hz']=carriers
 h.create_dataset('iq/baseband_complex32',data=np.asarray(values),compression='gzip',compression_opts=4)
