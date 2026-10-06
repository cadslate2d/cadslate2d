# Hershey fonts

`futural.jhf` is the Hershey "Simplex" font (Hershey Sans 1-stroke) in James Hurt's `.jhf` format, as distributed in the public `hershey-fonts` collection. The Hershey fonts were created by Dr. A. V. Hershey at the U.S. National Bureau of Standards and are free for any use, commercial or otherwise, under the terms of the original distribution, reproduced here:

> This distribution of the Hershey Fonts may be used by anyone for any purpose, commercial or otherwise, providing that:
>
> 1. The following acknowledgements must be distributed with the font data:
>    - The Hershey Fonts were originally created by Dr. A. V. Hershey while working at the U. S. National Bureau of Standards.
>    - The format of the Font data in this distribution was originally created by James Hurt, Cognition, Inc., 900 Technology Park Drive, Billerica, MA 01821 (mit-eddie!ci-dandelion!hurt).
> 2. The font data in this distribution may be converted into any other format *EXCEPT* the format distributed by the U.S. NTIS (which organization holds the rights to the distribution and use of the font data in that particular format). Not that anybody would really *want* to use their format… each point is described in eight bytes as "xxx yyy:", where xxx and yyy are the coordinate values as ASCII numbers.

Format: each line is one glyph — five characters of glyph number, three of vertex-pair count, then the pairs, each a pair of characters whose values are `code − 'R'`. The first pair is the left and right hand positions; a pair ` R` is a pen lift. Y grows downward.
