# Inspiration

This project was inspired by Njord (openenc) by manimaul.

The thought behind this is to increase performance of the tile serving as much as possible, using a high performance server like Martin and doing as much injest-time calculation as possible. Due to the nature of updates to ENCs,this would mean less repeated computation.

## Tile Seeding

Njord was originally intended to be run at the end user, like on a raspberry pi on the boat. Personally, I am not a fan of this. I believe that tile seeding is the best way to do so. The problem here is due to the nature of MVT, to seed all tiles in the US up to zoom level 14 would be unreasonable, especially at the high tile fetch times of njord. The solution to this was built into the architecture of njord, it calculates an ideal zoom level (based on scale) for each S-57 chart and only serves that data from that zoom level and lower unless there is a chart available at a lower zoom level, which it would then serve in a cascading series. The point of this would be to reduce the massive size of a worldwide MVT into something much more manageable, while still retaining some of the performance incentives of MVT and the charts detail.

The one problem is that most implementations to serve MVT from the directory structure (or mbtiles/pmtiles) do not automatically do this kind of fallback to overzooming when a tile at a specific zoom is not found (404). This will be a change down the road, but I am reasonably confident it will not be difficult.

## Running local

Even without tile seeding, manimaul mentioned that the nature of the JVM was a problem due to performance limitations on raspberry pi. His plan was to switch to kotlin native, but rust seemed faster and I am more familiar with it. That being said, his implementation can still change and that would work fine, these are not necessarily two fully overlapping projects and that is good.

The rust change brought me to martin, which is a completely seperate project but realized that the frontend and injest portions of this project can be seperated. This decoupling makes the architecture significantly easier to work with, and more flexible if someone wanted to run tileserver-gl or some other tileserver instead.

## Cloud Hosted

This is my though in my head, where one person (me) would host Martin connected to a private postgis instance. Then every day or so I would run S-57 updates and add them into the database with the rust code. Martin would then be the frontend (which I have found out is read-only so no risks there besides too much usage), and any administrative work would be done behind the scenes completely (I would likely prefer to keep it command-line).

In addition to that, I could keep an updated zip file of the sparse MVT directory structure that was described above in tile seeding, so that people can directly download the charts. This I think is important, since peace of mind in knowing you have all of your charts is ideal. This could be hosted anywhere, even google drive to start off.

## Framework

In my head, the ideal use for this is to rarely run the custom code from this project. This should be used to only import S-57 data and that means most of the time it lies dormant with only Martin, and the other time it is just adding in updates. This would be the case locally or cloud hosted and run every week or so.

Basically:

S-57 Updates -> Rust import -> PostGIS Database -> Martin -> (optional) Tile seeding update

## Conclusions

I do not know if this project will be any good, I know a lot of the work I will end up doing is simply porting over the njord logic to rust. However, in my usage of Njord it really broke down for the use cases I wanted, so at the very least it will work for me.
