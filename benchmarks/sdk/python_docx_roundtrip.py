# SPDX-License-Identifier: Apache-2.0
"""python-docx's round trip, for the SDK suite: open, touch nothing, save.

    python python_docx_roundtrip.py IN OUT
"""
import sys

import docx  # python-docx, installed by whoever runs the suite

document = docx.Document(sys.argv[1])
document.save(sys.argv[2])
