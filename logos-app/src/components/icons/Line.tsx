import React from "react";
import type { SVGProps } from "react";

export interface LineProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Line = React.forwardRef<SVGSVGElement, LineProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M0 288L640 288L640 352L0 352L0 288z"/>
      </svg>
    );
  }
);

Line.displayName = "Line";

export default Line;
