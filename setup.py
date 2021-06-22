import setuptools

#Genrate long description using readme file
with open("README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()

ext_modules = []
cSources = ['src/chirpsounder2/cSources/chirp_downconvert.c']
cLibraries = []
down_convert_module = setuptools.Extension(
            #Where to store the .so file
            name='chirpsounder2.libdownconvert',
            #Libraries used
            libraries=cLibraries,
            extra_compile_args = ['-pthread'],
            #Path to C source files, relative to repo root
            sources=cSources,
            )

setuptools.setup(
    name="ChirpSounder",
    version="2.0.0",
    author="Juha Vierinen, Vetle Hofsøy-Woie",
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
        'numpy',
        'pkgconfig',
        'setuptools',
        'h5py',
        'packaging',
        'tz',
        'matplotlib',
        'scipy',
        'digital_rf',
    ],
    package_dir={"": "src"},
    packages=setuptools.find_packages(where="src"),
    python_requires=">=3.6",
    ext_modules=ext_modules,
)
