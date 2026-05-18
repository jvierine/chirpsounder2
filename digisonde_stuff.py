import numpy as n

def digisonde_code_pair():
    """
    Return the 16-bit complementary code pair used by the Digisonde receiver.
    """
    b0=n.array([1,1,-1,1,1,1,1,-1,-1,1,1,1,-1,1,-1,-1],dtype=n.complex64)
    b1=n.array([-1,-1,1,-1,-1,-1,-1,1,-1,1,1,1,-1,1,-1,-1],dtype=n.complex64)
    return b0,b1


def complementary_code_autocorrelations():
    """
    Return lags and aperiodic autocorrelations for the Digisonde complementary
    code pair.
    """
    b0,b1=digisonde_code_pair()
    ac0=n.correlate(b0,b0,mode="full").real
    ac1=n.correlate(b1,b1,mode="full").real
    lags=n.arange(-(len(b0)-1),len(b0))
    return lags,ac0,ac1,ac0+ac1


def plot_complementary_code_autocorrelations(ofname="digisonde_complementary_code_autocorrelation.png"):
    """
    Plot the autocorrelations of the Digisonde complementary codes and their sum.
    """
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    lags,ac0,ac1,ac_sum=complementary_code_autocorrelations()

    fig,axes=plt.subplots(3,1,figsize=(7.2,6.4),sharex=True,constrained_layout=True)
    panels=[
        (ac0,"Code 1 autocorrelation"),
        (ac1,"Code 2 autocorrelation"),
        (ac_sum,"Sum: sidelobes cancel")
    ]
    for ax,(ac,title) in zip(axes,panels):
        ax.axhline(0,color="0.75",linewidth=0.8)
        ax.vlines(lags,0,ac,color="tab:blue",linewidth=1.4)
        ax.plot(lags,ac,"o",color="tab:blue",markersize=4)
        ax.set_ylabel("Amplitude")
        ax.set_title(title)
        ax.grid(True,axis="y",alpha=0.25)

    axes[-1].set_xlabel("Lag (code bits)")
    axes[-1].set_xticks(lags[::2])
    axes[-1].set_xlim(lags[0]-1,lags[-1]+1)
    fig.suptitle("Digisonde 16-bit Complementary Code Autocorrelations")
    fig.savefig(ofname,dpi=200)
    plt.close(fig)
    return ofname


def complementary_code(sr=50e3,ipp=10e-3,L=1.0/30e3,mode=0,all_codes=True):
    """
    Return a 16-bit complementary code pair with bit length L. 
    - resample to required sample rate

    mode=0  b0,b1,b0,b1,...
    mode=1  b0,b1,-b0,-b1,...
    mode=2  b0,b1,b0,b1,-b0,-b1,-b0,-b1,...
    """
    b0,b1=digisonde_code_pair()
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


if __name__ == "__main__":
    import argparse
    parser=argparse.ArgumentParser(description="Digisonde code utilities")
    parser.add_argument("--plot-complementary-code-autocorrelation",
                        default=None,
                        metavar="PNG",
                        help="Save a plot of the complementary-code autocorrelations.")
    args=parser.parse_args()

    if args.plot_complementary_code_autocorrelation is not None:
        ofname=plot_complementary_code_autocorrelations(args.plot_complementary_code_autocorrelation)
        print("wrote %s"%(ofname))
