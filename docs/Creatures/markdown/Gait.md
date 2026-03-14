# Gait

A baby maleBanana Norn's default gait.ACreatures 1norn's head sprite.ACreatures 1norn's body sprite.A **gait** is a series of [poses](https://creatures.wiki/Pose) that define a [creature](https://creatures.wiki/Creature)'s walking animations. They are defined in the gait genes in its [genome](https://creatures.wiki/Genome). Special gaits are used in certain circumstances as part of creatures' behavior, for example [mating dances](https://creatures.wiki/Lek) or [inebriation](https://creatures.wiki/Alcohol).

The gait to use is selected based on the current strongest [gait receptor locus](https://creatures.wiki/Biochemistry#Receptors).

The animation is interpolated smoothly between each pose defined in the gait, tilting each body part closer to the desired angle each tick until the pose is reached. In this sense, the poses can be thought of as a sequence of [key frames](https://en.wikipedia.org/wiki/Key_frame) that are used to generate the full animation. The amount of tilt required between each pose is also used to update the [muscles emitter locus](https://creatures.wiki/Biochemistry#Emitters) so that the biochemistry system knows how much energy is being used.

The creature will move based on the location of its feet during each animation frame, giving it a smooth, realistic walking animation.

## Creatures 1 gait strings

Gait strings are defined similarly to a normal [animation string](https://creatures.wiki/ANIM), but use two-digit numbers to refer to pose indexes instead of single-digits.

For example, the gait string `13141516R` defines a sequence of four poses, with indexes `13`, `14`, `15`, and `16`, which repeat until another gait is chosen.

## Creatures 1 pose strings

Pose strings are 15-character long strings, where each character specifies the angle for a certain body part, or a dynamic angle based on the current pose or target of attention.

The order of angles in the string is: [direction](https://creatures.wiki/DIRN), head, body, left thigh, left shin, left foot, right thigh, right shin, right foot, left humerus (upper arm), left radius (forearm and hand), right humerus, right radius, tail root, and tail tip.

There are seven possible angles for [direction](https://creatures.wiki/DIRN):

- `0` north (away from screen)
- `1` south (toward screen)
- `2` east (player right)
- `3` west (player left)
- `X` keep the current direction
- `?` turn the creature's head, then body parts, in the direction of [_IT_](https://creatures.wiki/IT)
- `!` turn the creature's head, then body parts, away from [_IT_](https://creatures.wiki/IT)

And the four angles `0`, `1`, `2`, and `3` for other body parts, plus two special angles:

- `X` keep the current angle
- `?` for a head, means angle the head towards _IT_

The sprite to use is determined based on the angle of the part and current direction the creature is facing. Each body part's sprite file has four right-facing images, following by four left-facing images, then one image facing the camera and one image facing away. Additionally, sprite files for heads have three additional images facing the camera for happy, sad, and angry expressions, and then the entire set of images is repeated with eyes closed.

For example, the pose string `?201221010012XX` is interpreted as:

## Creatures 1 gait example

In [Creatures 1](https://creatures.wiki/Creatures_1), the normal norn gait is `13141516R`, used when no other gait loci are being stimulated. This gait defines a sequence of four poses, with indexes `13`, `14`, `15`, and `16`, infinitely repeating.

The poses associated with these indexes are:

- `?201221010012XX` (pose 13)
- `?201013221101XX` (pose 14)
- `?101011221200XX` (pose 15)
- `?203221010111XX` (pose 16)

After interpolation, the full animation becomes:

- `?201221010012XX` (pose 13, uses 1 unit of energy when looping from end)
- `?XXX112121101XX` (interpolated, used 9 units of energy)
- `?201013221101XX` (pose 14, used 3 units of energy)
- `?1XXXX2XXX2X0XX` (interpolated, used 4 units of energy)
- `?101011221200XX` (pose 15, used 1 unit of energy)
- `?2X212X110111XX` (interpolated, used 10 units of energy)
- `?203221010111XX` (pose 16, used 3 units of energy)
- `?XX2XXXXXX0X2XX` (interpolated back towards the first pose, used 3 units of energy)

## See also

- [NY Walker](https://creatures.wiki/NY_Walker), a named mutation in the gait genes.
