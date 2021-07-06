import setuptools
from setuptools.command.install import install
import sys
import os
import subprocess

#Genrate long description using readme file
with open("README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()

def get_bin_path():
    """Used to work out path to install compiled binaries to."""
    if hasattr(sys, 'real_prefix') or os.getuid == 0:
        return sys.prefix

    if hasattr(sys, 'base_prefix') and sys.base_prefix != sys.prefix:
        return sys.prefix

    if 'conda' in sys.prefix:
        return sys.prefix

    return os.path.expanduser('~')+"/.local"

ext_modules = []
down_convert_Sources = ['src/chirpsounder/cSources/chirp_downconvert.c']
down_convert_Libraries = []
down_convert_Include = []
down_convert_module = setuptools.Extension(
            #Where to store the .so file
            name='chirpsounder.libdownconvert',
            #Libraries used
            include_dirs=down_convert_Include,
            libraries=down_convert_Libraries,
            extra_compile_args = ['-pthread'],
            #Path to C source files, relative to repo root
            sources=down_convert_Sources,
            )
            
ext_modules.append(down_convert_module)


class customInstall(install):
    def compile_and_install_rx_uhd(self):
        print('Compiling rx_uhd')
        src_file = './src/chirpsounder/cSources/rx_uhd.cpp'
        bin_path = get_bin_path() + '/bin/rx_uhd'
        print('Compiling executable into :' + bin_path)
        os.system('make INSTALL_PATH='+bin_path)

    def run(self):
        self.compile_and_install_rx_uhd()
        super().run()

setuptools.setup(
    name="ChirpSounder",
    version="2.0.3",
    author="Juha Vierinen",
    author_email="juha-pekka.vierinen@uit.no",
    description="Detect chirp sounders and over the horizon transmissions",
    long_description=long_description,
    long_description_content_type="text/markdown",
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Science/Research",
        "Natural Language :: English",
        "Programming Language :: Python :: 3",
    ],
    install_requires=[
        'mako',
        'numpy>=1.17.5',
        'pkgconfig',
        'setuptools',
        'h5py',
        'packaging',
        'tz',
        'matplotlib',
        'scipy',
        'digital_rf',
        'mpi4py',
    ],
    # extras_requires={
    #     'with-pyfftw':'pyfftw',
    # },
    scripts=[
        'bin/detect_chirps',
        'bin/calc_ionograms',
        'bin/plot_ionograms',
        'bin/plot_rf_spec',
    ],
    cmdclass={'install' : customInstall},
    package_dir={"": "src"},
    packages=setuptools.find_packages(where="src"),
    python_requires=">=3.6",
    ext_modules=ext_modules,
)
