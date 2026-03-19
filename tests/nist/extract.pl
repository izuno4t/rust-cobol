#!/usr/bin/env perl
# extract.pl — Extract individual COBOL test programs from newcob.val
#
# Replaces EXEC85 (which is itself a COBOL program).
# Reads newcob.val and splits it into individual .cob files by module.
#
# Usage: perl extract.pl <newcob.val> [output-dir]
#
# Output structure:
#   output-dir/NC/NC101A.cob
#   output-dir/NC/NC102A.cob
#   ...
#   output-dir/SQ/SQ101A.cob
#   ...

use strict;
use warnings;
use File::Path qw(make_path);
use File::Basename;

my $input  = shift or die "Usage: $0 <newcob.val> [output-dir]\n";
my $outdir = shift || "programs";

open(my $fh, '<', $input) or die "Cannot open $input: $!\n";

my $current_module = "";
my $current_name   = "";
my $current_fh;
my $program_count  = 0;
my %module_counts;

while (my $line = <$fh>) {
    chomp $line;

    # Detect program header lines
    # Format varies but typically: "      *HEADER,COBC85 4.2 85/01/01,NC101A."
    # or lines starting with specific markers
    if ($line =~ /^\s*\*HEADER/) {
        # Close previous file
        if ($current_fh) {
            close($current_fh);
        }

        # Extract program name from header
        # Pattern: *HEADER,<suite-info>,<program-name>.
        if ($line =~ /,\s*([A-Z]{2}\d{3}[A-Z])\s*\.?\s*$/) {
            $current_name = $1;
            $current_module = substr($current_name, 0, 2);
        } elsif ($line =~ /,\s*([A-Z]{2}\d{3}[A-Z0-9\-]+)\s*\.?\s*$/) {
            $current_name = $1;
            $current_module = substr($current_name, 0, 2);
        } else {
            warn "Could not parse program name from header: $line\n";
            $current_name = "";
            next;
        }

        # Create module directory
        my $mod_dir = "$outdir/$current_module";
        make_path($mod_dir) unless -d $mod_dir;

        # Open output file
        my $outfile = "$mod_dir/$current_name.cob";
        open($current_fh, '>', $outfile) or die "Cannot create $outfile: $!\n";
        $program_count++;
        $module_counts{$current_module}++;

        next;  # Don't write the *HEADER line itself
    }

    # Skip other control lines
    next if $line =~ /^\s*\*END/;

    # Write source line to current program file
    if ($current_fh && $current_name ne "") {
        print $current_fh "$line\n";
    }
}

# Close last file
if ($current_fh) {
    close($current_fh);
}

close($fh);

# Summary
print "Extracted $program_count programs to $outdir/\n";
print "Modules:\n";
for my $mod (sort keys %module_counts) {
    printf "  %-4s: %3d programs\n", $mod, $module_counts{$mod};
}
