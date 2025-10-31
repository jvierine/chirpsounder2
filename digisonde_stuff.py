import numpy as n

def complementary_code(sr=50e3,ipp=10e-3,L=1.0/30e3,mode=0,all_codes=True):
    """
    Return a 16-bit complementary code pair with bit length L. 
    - resample to required sample rate

    mode=0  b0,b1,b0,b1,...
    mode=1  b0,b1,-b0,-b1,...
    mode=2  b0,b1,b0,b1,-b0,-b1,-b0,-b1,...
    """
    b0=n.array([1,1,-1,1,1,1,1,-1,-1,1,1,1,-1,1,-1,-1],dtype=n.complex64)
    b1=n.array([-1,-1,1,-1,-1,-1,-1,1,-1,1,1,1,-1,1,-1,-1],dtype=n.complex64)
    b2=-1*n.copy(b0)
    b3=-1*n.copy(b1)
    zc=n.array([0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],dtype=n.complex64)
    n_samples=int(8*ipp*sr)
    t_max=n_samples/sr    

    # time (s)
    t=n.arange(n_samples)/sr - 16*L
    
    if mode==0: # o/x 
        codes=[b0,b1]
        modes=[0,0]
    elif mode==1: # o/x flip
        codes=[b0,b1,b2,b3]
        modes=[0,0,0,0]
    elif mode==2: # o,x interleaved
        modes=[0,0,1,1]
        if all_codes:
            codes=[b0,b1,b0,b1]
            
        else:
            codes=[b0,b1,zc,zc]
    elif mode==3:  # o,x,o,x interleaved, flip
        if all_codes:        
            codes=[b0,b1,b0,b1,b2,b3,b2,b3]
        else:
            codes=[b0,b1,zc,zc,b2,b3,zc,zc]
        modes=[0,0,1,1,0,0,1,1]

    n_codes=len(codes)
        
    n_ipp=int(n.floor(t_max/ipp))
    
    z=n.zeros(n_samples,dtype=n.complex64)
    for i in range(n_ipp):
        idx=n.where((t >i*ipp)&(t < (i+1)*ipp))[0]
        t0=n.fmod(t[idx],ipp)
        
        tx_idx=n.where( (t0 <= (L*16.0)) & ( t0 > 0.0 ))[0]
        orig_idx=idx[tx_idx]
        code_idx = i%(n_codes)
        code = codes[code_idx]
        z[orig_idx]=code[n.array(n.floor(t0[tx_idx]/L),dtype=int)]
    
    return(z,codes,modes)
