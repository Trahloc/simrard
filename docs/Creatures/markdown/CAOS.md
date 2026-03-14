# CAOS

**CAOS** (**C**reatures **A**gent **O**bject **S**cript) is the generic name for the embedded scripting language used in all games in the [Creatures series](https://creatures.wiki/Creatures_series). Note that the details of CAOS vary from [engine](https://creatures.wiki/Engine) to engine, but all incarnations have some overarching similarities. CAOS is used to make objects such as [COBs](https://creatures.wiki/COB) or [agents](https://creatures.wiki/Agent), and even controls breeding patterns in [worlds](https://creatures.wiki/World) - there are many [tutorials](https://creatures.wiki/Tutorial) (see [External links](https://creatures.wiki/CAOS#External_links)) which can help you to learn the CAOS language.

Looking for a quick fix? Here are someC3/DS CAOS Codes.CAOS is itself the name of a CAOS command! SeeCAOS (command).## Overview

CAOS is a register-based language, with a set of opcodes with fixed argument lists. Local variables are not supported. The basic unit of CAOS code is the command which can be injected and executed by the engine. However, CAOS is mostly used in [scripts](https://creatures.wiki/Script), blocks of code identified by four integers - three identifying the [object for which the script applies](https://creatures.wiki/Class_number) (0 may be used as a wildcard), and one identifying the [event](https://creatures.wiki/Event) the script triggers on. Those form the backbone of the actual game, declaring the different forms of interactions between the different [COBs](https://creatures.wiki/COB) or [agents](https://creatures.wiki/Agent) (a term for the programming objects introduced with [Creatures 3](https://creatures.wiki/Creatures_3) - every [gadget](https://creatures.wiki/Gadget) or even [creature](https://creatures.wiki/Creature) in the [world](https://creatures.wiki/World) is an [agent](https://creatures.wiki/Agent)) by defining [actions](https://creatures.wiki/Action) to be taken when certain events happen (from time triggered ones to [collisions with other objects](https://creatures.wiki/Physics) or even clicks).

Each script may contain [subroutines](https://creatures.wiki/Subroutine); however as these cannot be shared and do not have local variables, they are little more than a convenience.

Objects in Creatures are written using an [object-oriented programming](https://en.wikipedia.org/wiki/object-oriented_programming) approach.

CAOS scripts have 100 registers of the form [VA*xx*](https://creatures.wiki/VAxx) to use to hold temporary variables. In addition, an object may be selected into the special register [TARG](https://creatures.wiki/TARG), and then 100 [attributes](https://creatures.wiki/Attributes) of the object may be accessed with registers of the form [OV*xx*](https://creatures.wiki/OVxx). In most cases an object must be selected into TARG before it is acted upon. Using the command [AVAR](https://creatures.wiki/AVAR) to access the OV*xx* variables of an agent without having to fiddle with TARG, one can implement simple arrays.

Note that while [Creatures](https://creatures.wiki/Creatures) and [Creatures 2](https://creatures.wiki/Creatures_2), for performance reasons, use unsigned bytes (0-255) as integer types, later versions of the engine use 'normal' 32-bit integers (which can be very large) as well as floating point numbers (eg: 3.141593).

## Lists of CAOS Commands by Game

*Note: These are incomplete.*

- [Creatures 1](https://creatures.wiki/Category:C1_CAOS_Commands)
- [Creatures 2](https://creatures.wiki/Category:C2_CAOS_Commands)
- [Creatures 3](https://creatures.wiki/Category:C3_CAOS_Commands)

Also all C3 CAOS commands listed in a batch file

- [C3CAOSLister](https://creatures.wiki/CCLOCFC3)
- [Statistics](https://creatures.wiki/Statistics) of commands used in the games

## History of CAOS versions

- [Creatures](https://creatures.wiki/Creatures): Initial incarnation of CAOS.
- [Creatures 2](https://creatures.wiki/Creatures_2): Improved game [engine](https://creatures.wiki/Engine). CAOS now supports [physics](https://creatures.wiki/Physics).
- [Creatures Adventures](https://creatures.wiki/Creatures_Adventures): New game engine - the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine). Support for strings and file I/O is added. CAOS exceptions are much less likely to cause engine crashes.
- [Creatures 3](https://creatures.wiki/Creatures_3): Incremental improvements and changes to [Creatures Adventures](https://creatures.wiki/Creatures_Adventures) engine.
- [Docking Station](https://creatures.wiki/Docking_Station): Incremental improvements to [Creatures 3](https://creatures.wiki/Creatures_3) engine. Includes networking support ([Babel](https://creatures.wiki/Babel)).
- [Sea-Monkeys](https://creatures.wiki/Sea-Monkeys): Incremental improvements and changes to [Docking Station](https://creatures.wiki/Docking_Station) engine, without DS's [Babel](https://creatures.wiki/Babel).

## Full commented scripts

- [Creatures 1 scripts](https://creatures.wiki/Creatures_1_scripts)
- [Creatures 2 scripts](https://creatures.wiki/Creatures_2_scripts)
- [Creatures 3/DS scripts](https://creatures.wiki/Creatures_3/DS_scripts)

## Tutorials

- [Category:Tutorials](https://creatures.wiki/Category:Tutorials)

## External links

- [Creatures 1 CAOS guide (PDF)](https://web.archive.org/web/20170814234719/http://www.gamewareeurope.com/GWDev/downloads/cdn/creatures_caos_guide.pdf)
- [Creatures 2 CAOS guide (PDF)](https://web.archive.org/web/20071015154335/http://www.gamewaredevelopment.co.uk/downloads/cdn/C2CAOSGuide.pdf)
- [How to generate the Creatures 3 CAOS guide](https://web.archive.org/web/20170814225722/http://www.gamewareeurope.com/GWDev/cdn/cdn_more.php?CDN_article_id=27)
- [Getting Started with CAOS - C3/DS](https://web.archive.org/web/20140205202257/http://www.gamewaredevelopment.com/cdn/CDN_more.php?CDN_article_id=112)
- [AquaShee's CAOS Chaos - C3/DS](https://creaturescaves.com/community.php?category=&searchFor=CAOS+Chaos+%7C&section=Resources)
- [Thoughts for writing agents that are relatively nice](https://web.archive.org/web/20170625004638/http://mobilefieldbase.com/creatures/garbage.html)
- [Discussion on CAOS as image-based programming language, and history of CAOS](http://lambda-the-ultimate.org/node/view/455)
- [Control flow](https://en.wikipedia.org/wiki/Control_flow)
- [A comparison of Creatures Village, C3, DS, and the Virtual Sea-Monkeys CAOS commands](https://hello-robotto.dreamwidth.org/4230.html)
